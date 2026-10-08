//! Device tree parsing utilities using `dtb-walker`.
//!
//! Provides a [`Devicetree`] wrapper for extracting boot information
//! (bootargs, memory regions, initrd, timebase frequency) and a
//! [`NodeInfo`] struct used by the driver builder to collect per-node
//! properties during a DTB walk.

use crate::{DeviceError, DeviceResult, PhysAddr, VirtAddr};
use alloc::{string::String, vec::Vec};
use core::ops::Range;
use dtb_walker::{Dtb, DtbObj, Property, Str, WalkOperation::*};

/// A unified representation of the `interrupts` and `interrupts_extended`
/// properties for any interrupt generating device.
pub type InterruptsProp = Vec<u32>;

/// Boot information extracted from the device tree in a single walk.
pub struct Devicetree {
    bootargs: Option<String>,
    timebase_frequency: Option<u32>,
    initrd_start: Option<usize>,
    initrd_end: Option<usize>,
    memory_regions: Vec<Range<PhysAddr>>,
    /// Raw DTB pointer, kept for the driver builder's walk.
    dtb_vaddr: VirtAddr,
}

/// Collected properties for a single device tree node.
///
/// Used by [`DevicetreeDriverBuilder`](crate::builder::DevicetreeDriverBuilder)
/// during its walk to accumulate per-node data before creating drivers.
#[derive(Default)]
pub struct NodeInfo {
    /// Node name (e.g., "uart@10000000").
    pub name: String,
    /// Compatible strings (from the "compatible" property).
    pub compatible: Vec<String>,
    /// Base address and size from the "reg" property.
    pub reg: Option<(u64, u64)>,
    /// Whether this node has the "interrupt-controller" flag.
    pub is_interrupt_controller: bool,
    /// The "phandle" value, if present.
    pub phandle: Option<u32>,
    /// The "#interrupt-cells" value, if present.
    pub interrupt_cells: Option<u32>,
    /// The "interrupts-extended" property as raw u32 cells.
    pub interrupts_extended: Option<Vec<u32>>,
    /// The "interrupts" property as raw u32 cells (without parent prepended).
    pub interrupts: Option<Vec<u32>>,
    /// The "interrupt-parent" value, if present on this node.
    pub interrupt_parent: Option<u32>,
}

/// Some properties inherited from ancestor nodes.
///
/// About the notion: cell, see <https://elinux.org/Device_Tree_Usage#How_Addressing_Works>.
#[derive(Clone, Copy, Debug, Default)]
pub struct InheritProps {
    /// The `interrupt-parent` property inherited from ancestor nodes.
    pub interrupt_parent: u32,
}

impl NodeInfo {
    /// Build the effective `InterruptsProp` for this node, considering
    /// inherited interrupt-parent.
    pub fn effective_interrupts(&self, inherited: &InheritProps) -> InterruptsProp {
        if let Some(ref ext) = self.interrupts_extended {
            ext.clone()
        } else if let Some(ref irqs) = self.interrupts {
            let parent = self.interrupt_parent.unwrap_or(inherited.interrupt_parent);
            if parent > 0 {
                let mut ret = Vec::with_capacity(1 + irqs.len());
                ret.push(parent);
                ret.extend_from_slice(irqs);
                ret
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }

    /// Check if the compatible list contains a given string.
    pub fn compatible_contains(&self, s: &str) -> bool {
        self.compatible.iter().any(|c| c == s)
    }

    /// Check if any compatible string ends with the given suffix.
    pub fn compatible_any_ends_with(&self, suffix: &str) -> bool {
        self.compatible.iter().any(|c| c.ends_with(suffix))
    }
}

/// Parse a big-endian byte slice as a u64 (4 or 8 bytes).
fn parse_u64(value: &[u8]) -> Option<usize> {
    match value.len() {
        8 => Some(u64::from_be_bytes(value[..8].try_into().ok()?) as usize),
        4 => Some(u32::from_be_bytes(value[..4].try_into().ok()?) as usize),
        _ => None,
    }
}

/// Parse a big-endian byte slice as a u32 (exactly 4 bytes).
fn parse_u32(value: &[u8]) -> Option<u32> {
    if value.len() >= 4 {
        Some(u32::from_be_bytes(value[..4].try_into().ok()?))
    } else {
        None
    }
}

/// Parse a byte slice as an array of big-endian u32 cells.
fn parse_cells(value: &[u8]) -> Vec<u32> {
    value
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_be_bytes(*c))
        .collect()
}

impl Devicetree {
    /// Load and parse the device tree blob from the given virtual address.
    ///
    /// Extracts boot information (bootargs, timebase, initrd, memory)
    /// in a single walk of the DTB.
    pub fn from(dtb_base_vaddr: VirtAddr) -> DeviceResult<Self> {
        info!("Loading device tree blob from {:#x}", dtb_base_vaddr);
        let dtb = unsafe {
            Dtb::from_raw_parts_filtered(dtb_base_vaddr as *const u8, |e| {
                // Accept common non-fatal header issues (misalignment, compat version)
                matches!(
                    e,
                    dtb_walker::HeaderError::Misaligned(4)
                        | dtb_walker::HeaderError::LastCompVersion(_)
                )
            })
        }
        .map_err(|e| {
            warn!(
                "device-tree: failed to load DTB @ {:#x}: {:?}",
                dtb_base_vaddr, e
            );
            DeviceError::InvalidParam
        })?;

        let mut result = Devicetree {
            bootargs: None,
            timebase_frequency: None,
            initrd_start: None,
            initrd_end: None,
            memory_regions: Vec::new(),
            dtb_vaddr: dtb_base_vaddr,
        };

        // Track which top-level node we're inside.
        let mut in_chosen = false;
        let mut in_cpus = false;
        let mut in_memory = false;

        dtb.walk(|ctx, obj| match obj {
            DtbObj::SubNode { name } => {
                if ctx.is_root() {
                    in_chosen = name.as_bytes() == b"chosen";
                    in_cpus = name.as_bytes() == b"cpus";
                    in_memory = name.as_bytes().starts_with(b"memory");
                    if in_chosen || in_cpus || in_memory {
                        return StepInto;
                    }
                }
                StepOver
            }
            DtbObj::Property(Property::Reg(reg)) => {
                if in_memory {
                    for range in reg {
                        result.memory_regions.push(range);
                    }
                }
                StepOver
            }
            DtbObj::Property(Property::General { name, value }) => {
                if in_chosen {
                    if name == Str::from("bootargs") {
                        if let Ok(s) = core::str::from_utf8(value) {
                            result.bootargs = Some(s.trim_end_matches('\0').into());
                        }
                    } else if name == Str::from("linux,initrd-start") {
                        result.initrd_start = parse_u64(value);
                    } else if name == Str::from("linux,initrd-end") {
                        result.initrd_end = parse_u64(value);
                    }
                } else if in_cpus && name == Str::from("timebase-frequency") {
                    result.timebase_frequency = parse_u32(value);
                }
                StepOver
            }
            _ => StepOver,
        });

        Ok(result)
    }

    /// Returns the `bootargs` property from the `/chosen` node.
    pub fn bootargs(&self) -> Option<&str> {
        self.bootargs.as_deref()
    }

    /// Returns the `timebase-frequency` property from the `/cpus` node.
    pub fn timebase_frequency(&self) -> Option<u32> {
        self.timebase_frequency
    }

    /// Returns the initrd address range from the `/chosen` node.
    pub fn initrd_region(&self) -> Option<Range<PhysAddr>> {
        let start = self.initrd_start?;
        let end = self.initrd_end?;
        Some(start..end)
    }

    /// Returns the physical memory regions from `/memory` nodes.
    pub fn memory_regions(&self) -> DeviceResult<Vec<Range<PhysAddr>>> {
        Ok(self.memory_regions.clone())
    }

    /// Get the raw DTB virtual address for use by the driver builder.
    pub fn dtb_vaddr(&self) -> VirtAddr {
        self.dtb_vaddr
    }

    /// Walk the DTB for driver probing.
    ///
    /// Calls `device_node_op` for each node that has a `compatible` property,
    /// providing the collected [`NodeInfo`] and inherited [`InheritProps`].
    pub fn walk_for_drivers<F>(&self, mut device_node_op: F)
    where
        F: FnMut(&NodeInfo, &InheritProps),
    {
        let dtb = unsafe {
            Dtb::from_raw_parts_filtered(self.dtb_vaddr as *const u8, |e| {
                matches!(
                    e,
                    dtb_walker::HeaderError::Misaligned(4)
                        | dtb_walker::HeaderError::LastCompVersion(_)
                )
            })
        };
        let dtb = match dtb {
            Ok(d) => d,
            Err(_) => return,
        };

        // We need to collect properties per-node. Since dtb-walker gives us
        // properties one at a time via callbacks, we accumulate them in a
        // stack of NodeInfo structs (one per nesting level).
        //
        // When we see a SubNode, we push a new NodeInfo. When we see properties,
        // we add them to the current (top) NodeInfo. When we step out of a node
        // (next SubNode at same level or end), we pop and process it.
        //
        // However, dtb-walker doesn't give us an explicit "end of node" signal.
        // Instead, we detect it when we see the next SubNode at the same or
        // higher level, or when the walk ends.

        let mut inherited_stack: Vec<InheritProps> = Vec::new();
        inherited_stack.push(InheritProps::default());

        let mut current_node: Option<NodeInfo> = None;
        let mut current_level: usize = 0;

        dtb.walk(|ctx, obj| {
            let level = ctx.level();

            match obj {
                DtbObj::SubNode { name } => {
                    // Process the previous node at this level or deeper
                    if let Some(node) = current_node.take() {
                        if !node.compatible.is_empty() {
                            let inherited = inherited_stack.last().copied().unwrap_or_default();
                            device_node_op(&node, &inherited);
                        }
                        // Update inherited props from the processed node
                        if let Some(ip) = node.interrupt_parent {
                            if let Some(last) = inherited_stack.last_mut() {
                                last.interrupt_parent = ip;
                            }
                        }
                    }

                    // Manage the inherited props stack
                    while inherited_stack.len() > level + 1 {
                        inherited_stack.pop();
                    }
                    if inherited_stack.len() <= level {
                        let parent = inherited_stack.last().copied().unwrap_or_default();
                        inherited_stack.push(parent);
                    }

                    // Start collecting a new node
                    let name_str = name.as_str().unwrap_or("").into();
                    current_node = Some(NodeInfo {
                        name: name_str,
                        ..Default::default()
                    });
                    current_level = level;

                    StepInto
                }
                DtbObj::Property(prop) => {
                    if let Some(ref mut node) = current_node {
                        match prop {
                            Property::Compatible(list) => {
                                for s in list {
                                    if let Ok(cs) = s.as_str() {
                                        node.compatible.push(cs.into());
                                    }
                                }
                            }
                            Property::PHandle(ph) => {
                                node.phandle = Some(ph.value());
                            }
                            Property::Reg(reg) => {
                                // Take the first range
                                if let Some(range) = reg.clone().next() {
                                    node.reg = Some((
                                        range.start as u64,
                                        (range.end - range.start) as u64,
                                    ));
                                }
                            }
                            Property::General { name, value } => {
                                if name == Str::from("interrupt-controller") {
                                    node.is_interrupt_controller = true;
                                } else if name == Str::from("#interrupt-cells") {
                                    // dtb-walker consumes this internally, but we
                                    // still need it for the driver builder. It may
                                    // not reach here. Handle both cases.
                                    node.interrupt_cells = parse_u32(value);
                                } else if name == Str::from("interrupts-extended") {
                                    node.interrupts_extended = Some(parse_cells(value));
                                } else if name == Str::from("interrupts") {
                                    node.interrupts = Some(parse_cells(value));
                                } else if name == Str::from("interrupt-parent") {
                                    node.interrupt_parent = parse_u32(value);
                                } else if name == Str::from("phandle") {
                                    // Fallback: dtb-walker may parse this as
                                    // Property::PHandle, but handle raw form too.
                                    if node.phandle.is_none() {
                                        node.phandle = parse_u32(value);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    StepOver
                }
            }
        });

        // Process the last node
        if let Some(node) = current_node.take() {
            if !node.compatible.is_empty() {
                let inherited = inherited_stack.last().copied().unwrap_or_default();
                device_node_op(&node, &inherited);
            }
        }
    }
}
