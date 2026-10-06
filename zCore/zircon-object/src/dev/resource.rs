use {crate::object::*, alloc::sync::Arc, bitflags::bitflags, numeric_enum_macro::numeric_enum};

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
        Err(ZxError::OUT_OF_RANGE)
    }

    /// Returns `Err(ZxError::INVALID_ARGS)` if the resource is not the root resource, and
    /// either it's flags or parameter `flags` contains `ResourceFlags::EXCLUSIVE`.
    pub fn check_exclusive(&self, flags: ResourceFlags) -> ZxResult {
        if self.kind != ResourceKind::ROOT
            && (self.flags.contains(ResourceFlags::EXCLUSIVE)
                || flags.contains(ResourceFlags::EXCLUSIVE))
        {
            Err(ZxError::INVALID_ARGS)
        } else {
            Ok(())
        }
    }

    /// Get information of the resource.
    pub fn get_info(&self) -> ResourceInfo {
        let name = self.base.name();
        let name = name.as_bytes();
        let mut name_vec = [0u8; 32];
        name_vec[..name.len()].clone_from_slice(name);
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
