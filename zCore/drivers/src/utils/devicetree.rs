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
    cpu_count: usize,
    /// Device addresses keyed by compatible string (first match wins).
    /// E.g., "arm,pl011" -> 0x0900_0000 for UART base address.
    device_addresses: alloc::collections::BTreeMap<String, usize>,
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

/// Scan raw DTB bytes to extract #interrupt-cells for each phandle.
///
/// dtb-walker consumes #interrupt-cells internally and doesn't deliver it
/// to the walk callback. We need it for the driver builder's interrupt
/// registration loop. This function scans the raw FDT structure block
/// to find phandle + #interrupt-cells pairs.
fn scan_interrupt_cells(dtb_vaddr: VirtAddr) -> alloc::collections::BTreeMap<u32, u32> {
    use alloc::collections::BTreeMap;

    let mut map = BTreeMap::new();

    // Parse the FDT header to find the structure block.
    let header = dtb_vaddr as *const u8;
    let read_u32 = |off: usize| -> u32 {
        unsafe {
            let p = header.add(off);
            u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
        }
    };

    let magic = read_u32(0);
    if magic != 0xd00dfeed {
        return map;
    }
    let totalsize = read_u32(4) as usize;
    let off_dt_struct = read_u32(8) as usize;
    let off_dt_strings = read_u32(12) as usize;

    let struct_base = dtb_vaddr + off_dt_struct;
    let strings_base = dtb_vaddr + off_dt_strings;
    let struct_end = dtb_vaddr + totalsize.min(off_dt_struct + 0x100000); // safety limit

    // Walk the structure block looking for phandle and #interrupt-cells
    // properties within the same node.
    let mut pos = struct_base;
    let mut current_phandle: Option<u32> = None;
    let mut current_intc_cells: Option<u32> = None;

    let read_u32_at = |addr: usize| -> u32 {
        unsafe {
            let p = addr as *const u8;
            u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
        }
    };

    // Get a null-terminated string from the strings block.
    let get_string = |nameoff: u32| -> &[u8] {
        let start = strings_base + nameoff as usize;
        let mut end = start;
        unsafe {
            while *(end as *const u8) != 0 && end < struct_end {
                end += 1;
            }
        }
        unsafe { core::slice::from_raw_parts(start as *const u8, end - start) }
    };

    while pos + 4 <= struct_end {
        let token = read_u32_at(pos);
        pos += 4;
        match token {
            1 => {
                // FDT_BEGIN_NODE: skip the name (null-terminated, 4-byte aligned)
                // Flush previous node
                if let (Some(ph), Some(cells)) = (current_phandle, current_intc_cells) {
                    map.insert(ph, cells);
                }
                current_phandle = None;
                current_intc_cells = None;

                // Skip node name
                while pos < struct_end {
                    if unsafe { *(pos as *const u8) } == 0 {
                        pos += 1;
                        break;
                    }
                    pos += 1;
                }
                // Align to 4 bytes
                pos = (pos + 3) & !3;
            }
            2 => {
                // FDT_END_NODE
                if let (Some(ph), Some(cells)) = (current_phandle, current_intc_cells) {
                    map.insert(ph, cells);
                }
                current_phandle = None;
                current_intc_cells = None;
            }
            3 => {
                // FDT_PROP: len (u32), nameoff (u32), value (len bytes, padded)
                if pos + 8 > struct_end {
                    break;
                }
                let len = read_u32_at(pos) as usize;
                let nameoff = read_u32_at(pos + 4);
                pos += 8;
                let value_start = pos;
                let padded_len = (len + 3) & !3;
                pos += padded_len;

                if pos > struct_end {
                    break;
                }

                let name = get_string(nameoff);
                if (name == b"phandle" || (name == b"linux,phandle" && current_phandle.is_none()))
                    && len == 4
                {
                    current_phandle = Some(read_u32_at(value_start));
                } else if name == b"#interrupt-cells" && len == 4 {
                    current_intc_cells = Some(read_u32_at(value_start));
                }
            }
            9 => break, // FDT_END
            4 => {}     // FDT_NOP
            _ => break, // Unknown token
        }
    }

    // Flush last node
    if let (Some(ph), Some(cells)) = (current_phandle, current_intc_cells) {
        map.insert(ph, cells);
    }

    map
}

/// Parse the hex address from a device tree node name like "pl011@9000000".
/// Returns `None` if there's no '@' or the address is not valid hex.
fn parse_node_addr(name: &[u8]) -> Option<usize> {
    let at_pos = name.iter().position(|&b| b == b'@')?;
    let hex = &name[at_pos + 1..];
    let s = core::str::from_utf8(hex).ok()?;
    usize::from_str_radix(s, 16).ok()
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
            cpu_count: 0,
            device_addresses: alloc::collections::BTreeMap::new(),
            dtb_vaddr: dtb_base_vaddr,
        };

        // Track context during the walk. We need to know which
        // top-level (or /soc child) node we're inside, and the
        // current node name (for extracting addresses like "pl011@9000000").
        let mut current_node: [u8; 64] = [0; 64];
        let mut current_node_len: usize = 0;

        dtb.walk(|ctx, obj| match obj {
            DtbObj::SubNode { name } => {
                let name_bytes = name.as_bytes();
                if ctx.is_root() {
                    // Save node name for property context
                    current_node_len = name_bytes.len().min(64);
                    current_node[..current_node_len]
                        .copy_from_slice(&name_bytes[..current_node_len]);
                    // Step into nodes we care about
                    if name_bytes.starts_with(b"chosen")
                        || name_bytes.starts_with(b"memory")
                        || name_bytes == b"cpus"
                        || name_bytes == b"soc"
                    {
                        return StepInto;
                    }
                    // Also step into top-level device nodes (e.g., pl011@, uart@, intc@)
                    if name_bytes.contains(&b'@') {
                        return StepInto;
                    }
                } else {
                    current_node_len = name_bytes.len().min(64);
                    current_node[..current_node_len]
                        .copy_from_slice(&name_bytes[..current_node_len]);

                    // Count CPU nodes
                    if name_bytes.starts_with(b"cpu@") {
                        result.cpu_count += 1;
                        return StepOver;
                    }
                    // Step into device nodes inside /soc
                    if name_bytes.contains(&b'@') {
                        return StepInto;
                    }
                }
                StepOver
            }
            DtbObj::Property(Property::Reg(reg)) => {
                let ctx_name = &current_node[..current_node_len];
                if ctx_name.starts_with(b"memory") {
                    for range in reg {
                        result.memory_regions.push(range);
                    }
                }
                StepOver
            }
            DtbObj::Property(Property::Compatible(list)) => {
                // Record device address from node name for each compatible string.
                // Node names like "pl011@9000000" encode the address after '@'.
                let ctx_name = &current_node[..current_node_len];
                if let Some(addr) = parse_node_addr(ctx_name) {
                    for s in list {
                        if let Ok(cs) = s.as_str() {
                            // Only record the first occurrence of each compatible string
                            let key = String::from(cs);
                            result.device_addresses.entry(key).or_insert(addr);
                        }
                    }
                }
                StepOver
            }
            DtbObj::Property(Property::General { name, value }) => {
                let ctx_name = &current_node[..current_node_len];
                if ctx_name.starts_with(b"chosen") {
                    if name == Str::from("bootargs") {
                        if let Ok(s) = core::str::from_utf8(value) {
                            result.bootargs = Some(s.trim_end_matches('\0').into());
                        }
                    } else if name == Str::from("linux,initrd-start") {
                        result.initrd_start = parse_u64(value);
                    } else if name == Str::from("linux,initrd-end") {
                        result.initrd_end = parse_u64(value);
                    }
                } else if ctx_name == b"cpus" && name == Str::from("timebase-frequency") {
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

    /// Returns the number of CPU nodes found in `/cpus`.
    pub fn cpu_count(&self) -> usize {
        self.cpu_count
    }

    /// Returns the base address of a device by its compatible string.
    ///
    /// The address is extracted from the device tree node name
    /// (e.g., "pl011@9000000" -> 0x9000000). Returns `None` if no
    /// device with the given compatible string was found.
    pub fn device_address(&self, compatible: &str) -> Option<usize> {
        self.device_addresses.get(compatible).copied()
    }

    /// Get the raw DTB virtual address for use by the driver builder.
    pub fn dtb_vaddr(&self) -> VirtAddr {
        self.dtb_vaddr
    }

    /// Walk the DTB for driver probing.
    ///
    /// Collects all device nodes into a flat list, then calls `device_node_op`
    /// for each one. Uses a two-pass approach:
    /// 1. Walk the DTB to collect all nodes with their properties
    /// 2. Process the collected nodes sequentially
    ///
    /// This avoids issues with dtb-walker consuming `#interrupt-cells`
    /// internally -- we scan the raw DTB bytes to extract it separately.
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

        // Pre-scan: extract #interrupt-cells for each phandle from the raw
        // DTB bytes. dtb-walker consumes this property internally and never
        // delivers it to the walk callback.
        let intc_cells_map = scan_interrupt_cells(self.dtb_vaddr);

        // Pass 1: Collect all nodes with their properties and depth levels.
        let mut nodes: Vec<(NodeInfo, usize)> = Vec::new(); // (node, depth)
        let mut current_node: Option<NodeInfo> = None;
        let mut current_level: usize = 0;

        dtb.walk(|ctx, obj| {
            let level = ctx.level();

            match obj {
                DtbObj::SubNode { name } => {
                    // Flush the previous node
                    if let Some(node) = current_node.take() {
                        nodes.push((node, current_level));
                    }

                    current_node = Some(NodeInfo {
                        name: name.as_str().unwrap_or("").into(),
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
                                    // dtb-walker normally consumes this, but
                                    // may deliver it if format is unexpected.
                                    node.interrupt_cells = parse_u32(value);
                                } else if name == Str::from("interrupts-extended") {
                                    node.interrupts_extended = Some(parse_cells(value));
                                } else if name == Str::from("interrupts") {
                                    node.interrupts = Some(parse_cells(value));
                                } else if name == Str::from("interrupt-parent") {
                                    node.interrupt_parent = parse_u32(value);
                                } else if name == Str::from("phandle") && node.phandle.is_none() {
                                    node.phandle = parse_u32(value);
                                }
                            }
                            _ => {}
                        }
                    }
                    StepOver
                }
            }
        });

        // Flush the last node
        if let Some(node) = current_node.take() {
            nodes.push((node, current_level));
        }

        // Pass 2: Resolve #interrupt-cells for interrupt controllers.
        // dtb-walker consumes this property internally, so we use the
        // pre-scanned map from the raw DTB bytes.
        for (node, _level) in &mut nodes {
            if node.is_interrupt_controller && node.interrupt_cells.is_none() {
                if let Some(ph) = node.phandle {
                    node.interrupt_cells = intc_cells_map.get(&ph).copied();
                }
            }
        }

        // Build interrupt-parent inheritance by depth.
        let mut inherited_stack: Vec<InheritProps> = alloc::vec![InheritProps::default()];

        // Collect root-level interrupt-parent if present.
        // (Root properties arrive before any SubNode in the DTB.)
        // We handle this by scanning the first few nodes at depth 0.

        for (node, level) in &nodes {
            // Trim stack to current level
            inherited_stack.truncate(inherited_stack.len().max(*level + 1));
            while inherited_stack.len() <= *level {
                let parent = inherited_stack.last().copied().unwrap_or_default();
                inherited_stack.push(parent);
            }

            let inherited = inherited_stack[*level];

            if !node.compatible.is_empty() {
                device_node_op(node, &inherited);
            }

            // If this node sets interrupt-parent, it applies to children
            // (next deeper level), not to siblings.
            if let Some(ip) = node.interrupt_parent {
                let child_level = *level + 1;
                let mut child_props = inherited;
                child_props.interrupt_parent = ip;
                if child_level < inherited_stack.len() {
                    inherited_stack[child_level] = child_props;
                } else {
                    inherited_stack.resize(child_level + 1, child_props);
                }
            }
        }
    }
}
