use {crate::object::*, alloc::sync::Arc, bitflags::bitflags, numeric_enum_macro::numeric_enum};

numeric_enum! {
    #[repr(u32)]
    /// ResourceKind definition from fuchsia/zircon/system/public/zircon/syscalls/resource.h
    #[allow(missing_docs)]
    #[allow(clippy::upper_case_acronyms)]
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum ResourceKind {
        MMIO = 0,
        IRQ = 1,
        IOPORT = 2,
        HYPERVISOR = 3,
        ROOT = 4,
        VMEX = 5,
        SMC = 6,
        COUNT = 7,
        /// System resource kind (used for power, debug, mexec, etc.)
        SYSTEM = 0x3F,
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

impl_kobject!(Resource);

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
    pub fn validate(&self, kind: ResourceKind) -> ZxResult {
        if self.kind == kind || self.kind == ResourceKind::ROOT {
            Ok(())
        } else {
            Err(ZxError::WRONG_TYPE)
        }
    }

    /// Validate the resource is the given kind or it is the root resource,
    /// and [addr, addr+len] is within the range of the resource.
    pub fn validate_ranged_resource(
        &self,
        kind: ResourceKind,
        addr: usize,
        len: usize,
    ) -> ZxResult {
        self.validate(kind)?;
        if addr >= self.addr && (addr + len) <= (self.addr + self.len) {
            Ok(())
        } else {
            Err(ZxError::OUT_OF_RANGE)
        }
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
// These match zircon/system/public/zircon/syscalls/resource.h.

/// Base for power control (reboot, shutdown).
pub const ZX_RSRC_SYSTEM_POWER_BASE: usize = 0;
/// Base for mexec (soft reboot / kexec).
pub const ZX_RSRC_SYSTEM_MEXEC_BASE: usize = 1;
/// Base for debug operations (debug_send_command, mtrace).
pub const ZX_RSRC_SYSTEM_DEBUG_BASE: usize = 2;
/// Base for MSI interrupt allocation.
pub const ZX_RSRC_SYSTEM_MSI_BASE: usize = 3;
/// Base for profile creation.
pub const ZX_RSRC_SYSTEM_PROFILE_BASE: usize = 4;
/// Base for kernel tracing (ktrace).
pub const ZX_RSRC_SYSTEM_TRACING_BASE: usize = 5;

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
