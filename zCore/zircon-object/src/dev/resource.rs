use {
    crate::object::*, alloc::sync::Arc, alloc::vec::Vec, bitflags::bitflags, lock::Mutex,
    numeric_enum_macro::numeric_enum,
};

/// Global registry of allocated resource ranges.
/// Tracks (kind, addr, len, exclusive) for overlap checking.
static RESOURCE_REGIONS: Mutex<Vec<ResourceRegion>> = Mutex::new(Vec::new());

#[derive(Clone)]
struct ResourceRegion {
    kind: u32,
    addr: usize,
    len: usize,
    #[allow(dead_code)]
    exclusive: bool,
    /// KoID of the owning Resource (for cleanup on drop).
    owner_koid: KoID,
}

impl ResourceRegion {
    fn overlaps(&self, kind: u32, addr: usize, len: usize) -> bool {
        if self.kind != kind {
            return false;
        }
        let self_end = self.addr.saturating_add(self.len);
        let other_end = addr.saturating_add(len);
        self.addr < other_end && addr < self_end
    }
}

numeric_enum! {
    #[repr(u32)]
    /// ResourceKind definition from fuchsia/zircon/system/public/zircon/syscalls/resource.h
    ///
    /// ABI values match Fuchsia exactly:
    ///   MMIO=0, IRQ=1, IOPORT=2, SMC=4, SYSTEM=5, COUNT=6
    ///
    /// ROOT (0x3F) is an internal-only value — not part of the
    /// userspace ABI.  HYPERVISOR and VMEX are system sub-resources
    /// (ZX_RSRC_SYSTEM_HYPERVISOR_BASE, ZX_RSRC_SYSTEM_VMEX_BASE),
    /// not separate top-level kinds.
    #[allow(missing_docs)]
    #[allow(clippy::upper_case_acronyms)]
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum ResourceKind {
        MMIO = 0,
        IRQ = 1,
        IOPORT = 2,
        // 3 is unused in Fuchsia ABI
        SMC = 4,
        SYSTEM = 5,
        COUNT = 6,
        /// Internal-only: the root resource that can create any
        /// sub-resource. Not exposed as a userspace ABI value.
        ROOT = 0x3F,
    }
}

bitflags! {
    /// Bits for Resource.flags.
    pub struct ResourceFlags: u32 {
        #[allow(clippy::identity_op)]
        /// Exclusive resource.
        const EXCLUSIVE      = 1 << 16;
    }
}

/// Address space rights and accounting.
pub struct Resource {
    base: KObjectBase,
    kind: ResourceKind,
    addr: usize,
    len: usize,
    flags: ResourceFlags,
}

impl_kobject!(Resource
    fn as_resource(&self) -> Option<&crate::dev::Resource> {
        Some(self)
    }
    fn on_zero_handles(&self) {
        self.unregister_region();
    }
);

impl Resource {
    /// Create a new `Resource`.
    pub fn create(
        name: &str,
        kind: ResourceKind,
        addr: usize,
        len: usize,
        flags: ResourceFlags,
    ) -> Arc<Self> {
        Arc::new(Resource {
            base: KObjectBase::with_name(name),
            kind,
            addr,
            len,
            flags,
        })
    }

    /// Validate the resource is the given kind or it is the root resource.
    ///
    /// Only ROOT bypasses kind checks.  A SYSTEM resource only matches
    /// if `kind == SYSTEM`.  All other resources must match exactly.
    pub fn validate(&self, kind: ResourceKind) -> ZxResult {
        if self.kind == kind || self.kind == ResourceKind::ROOT {
            Ok(())
        } else {
            Err(ZxError::WRONG_TYPE)
        }
    }

    /// Validate the resource is the given kind or it is the root resource,
    /// and [addr, addr+len] is within the range of the resource.
    ///
    /// Only ROOT bypasses range checks.  SYSTEM (and all other)
    /// resources must match kind AND cover the requested range.
    pub fn validate_ranged_resource(
        &self,
        kind: ResourceKind,
        addr: usize,
        len: usize,
    ) -> ZxResult {
        self.validate(kind)?;
        // ROOT resources allow any sub-range.
        if self.kind == ResourceKind::ROOT {
            return Ok(());
        }
        let req_end = addr.checked_add(len);
        let res_end = self.addr.checked_add(self.len);
        if let (Some(req_end), Some(res_end)) = (req_end, res_end) {
            if addr >= self.addr && req_end <= res_end {
                return Ok(());
            }
        }
        Err(ZxError::ACCESS_DENIED)
    }

    /// Check exclusive resource constraints against the global registry.
    ///
    /// Returns NOT_FOUND if:
    /// - Creating an exclusive resource that overlaps any existing resource
    /// - Creating any resource that overlaps an existing exclusive resource
    pub fn check_exclusive_overlap(
        kind: ResourceKind,
        addr: usize,
        len: usize,
        flags: ResourceFlags,
    ) -> ZxResult {
        let regions = RESOURCE_REGIONS.lock();
        let kind_raw = kind as u32;
        for region in regions.iter() {
            if region.overlaps(kind_raw, addr, len) {
                // Any overlap with an exclusive region is rejected.
                if region.exclusive || flags.contains(ResourceFlags::EXCLUSIVE) {
                    return Err(ZxError::NOT_FOUND);
                }
            }
        }
        Ok(())
    }

    /// Register this resource's range in the global registry.
    pub fn register_region(&self) {
        if self.kind == ResourceKind::ROOT || self.kind == ResourceKind::COUNT {
            return; // ROOT/COUNT don't participate in overlap tracking
        }
        let mut regions = RESOURCE_REGIONS.lock();
        regions.push(ResourceRegion {
            kind: self.kind as u32,
            addr: self.addr,
            len: self.len,
            exclusive: self.flags.contains(ResourceFlags::EXCLUSIVE),
            owner_koid: self.base.id,
        });
    }

    /// Unregister this resource's range from the global registry.
    fn unregister_region(&self) {
        let mut regions = RESOURCE_REGIONS.lock();
        regions.retain(|r| r.owner_koid != self.base.id);
    }

    /// Whether this resource has the EXCLUSIVE flag.
    pub fn is_exclusive(&self) -> bool {
        self.flags.contains(ResourceFlags::EXCLUSIVE)
    }

    /// Get information of the resource.
    pub fn get_info(&self) -> ResourceInfo {
        let name = self.base.name();
        let name = name.as_bytes();
        let mut name_vec = [0u8; 32];
        // Copy name, reserving last byte for NUL terminator.
        let copy_len = name.len().min(name_vec.len() - 1);
        name_vec[..copy_len].clone_from_slice(&name[..copy_len]);
        ResourceInfo {
            kind: self.kind as _,
            flags: self.flags.bits,
            base: self.addr as _,
            size: self.len as _,
            name: name_vec,
        }
    }
}

// System resource sub-resource base IDs (for validate_ranged_resource).
// Values match zircon/system/public/zircon/syscalls/resource.h exactly.

/// Base for hypervisor resource.
pub const ZX_RSRC_SYSTEM_HYPERVISOR_BASE: usize = 0;
/// Base for VMEX (VM-exec) resource.
pub const ZX_RSRC_SYSTEM_VMEX_BASE: usize = 1;
/// Base for debug operations (debug_send_command, mtrace).
pub const ZX_RSRC_SYSTEM_DEBUG_BASE: usize = 2;
/// Base for info resource.
pub const ZX_RSRC_SYSTEM_INFO_BASE: usize = 3;
/// Base for CPU resource.
pub const ZX_RSRC_SYSTEM_CPU_BASE: usize = 4;
/// Base for power control (reboot, shutdown).
pub const ZX_RSRC_SYSTEM_POWER_BASE: usize = 5;
/// Base for mexec (soft reboot / kexec).
pub const ZX_RSRC_SYSTEM_MEXEC_BASE: usize = 6;
/// Base for energy info resource.
pub const ZX_RSRC_SYSTEM_ENERGY_INFO_BASE: usize = 7;
/// Base for IOMMU resource.
pub const ZX_RSRC_SYSTEM_IOMMU_BASE: usize = 8;
// 9 is unused
/// Base for profile creation.
pub const ZX_RSRC_SYSTEM_PROFILE_BASE: usize = 10;
/// Base for MSI interrupt allocation.
pub const ZX_RSRC_SYSTEM_MSI_BASE: usize = 11;
/// Base for debuglog resource.
pub const ZX_RSRC_SYSTEM_DEBUGLOG_BASE: usize = 12;
/// Base for stall resource.
pub const ZX_RSRC_SYSTEM_STALL_BASE: usize = 13;
/// Base for kernel tracing (ktrace).
pub const ZX_RSRC_SYSTEM_TRACING_BASE: usize = 14;
/// Base for thread sampling.
pub const ZX_RSRC_SYSTEM_SAMPLING_BASE: usize = 15;

/// Information of a resource.
#[repr(C)]
#[derive(Default)]
pub struct ResourceInfo {
    kind: u32,
    flags: u32,
    base: u64,
    size: u64,
    name: [u8; 32], // should be [char; 32], but I cannot compile it
}
