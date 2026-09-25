//! Zircon Profile kernel object.
//!
//! A Profile encodes scheduling parameters that can be applied to
//! threads via `zx_object_set_profile()`.

use crate::object::*;
use alloc::sync::Arc;
use bitflags::bitflags;

/// A Profile kernel object encoding scheduling parameters.
pub struct Profile {
    base: KObjectBase,
    _counter: CountHelper,
    /// The profile configuration.
    pub info: ProfileInfo,
}

impl_kobject!(Profile);
define_count_helper!(Profile);

impl Profile {
    /// Create a new Profile with the given configuration.
    ///
    /// Validates the flags and scheduling parameters.
    pub fn create(info: ProfileInfo) -> ZxResult<Arc<Self>> {
        let flags = info.flags();
        let has_priority = flags.contains(ProfileInfoFlags::PRIORITY);
        let has_deadline = flags.contains(ProfileInfoFlags::DEADLINE);
        let has_memory = flags.contains(ProfileInfoFlags::MEMORY_PRIORITY);
        let has_critical = flags.contains(ProfileInfoFlags::CRITICAL);
        let has_no_inherit = flags.contains(ProfileInfoFlags::NO_INHERIT);

        // Must specify exactly one scheduling discipline (or memory priority)
        if has_memory {
            // Memory priority is incompatible with scheduling flags
            if has_priority || has_deadline {
                return Err(ZxError::INVALID_ARGS);
            }
        } else if has_priority && has_deadline {
            // Cannot combine PRIORITY and DEADLINE
            return Err(ZxError::INVALID_ARGS);
        } else if !has_priority && !has_deadline {
            // Must specify at least one discipline (unless CPU_MASK only)
            if !flags.contains(ProfileInfoFlags::CPU_MASK) {
                return Err(ZxError::INVALID_ARGS);
            }
        }

        // CRITICAL requires DEADLINE
        if has_critical && !has_deadline {
            return Err(ZxError::INVALID_ARGS);
        }

        // NO_INHERIT is incompatible with DEADLINE
        if has_no_inherit && has_deadline {
            return Err(ZxError::INVALID_ARGS);
        }

        // Validate priority range
        if has_priority && !(LOWEST_PRIORITY..=HIGHEST_PRIORITY).contains(&info.priority) {
            return Err(ZxError::INVALID_ARGS);
        }

        Ok(Arc::new(Profile {
            base: KObjectBase::default(),
            _counter: CountHelper::new(),
            info,
        }))
    }
}

bitflags! {
    /// Flags for `ProfileInfo`.
    pub struct ProfileInfoFlags: u32 {
        /// Fair scheduling with the given priority.
        const PRIORITY        = 1 << 0;
        /// Set CPU affinity mask.
        const CPU_MASK        = 1 << 1;
        /// Deadline scheduling.
        const DEADLINE        = 1 << 2;
        /// Do not participate in priority inheritance.
        const NO_INHERIT      = 1 << 3;
        /// Memory priority (for VMARs).
        const MEMORY_PRIORITY = 1 << 4;
        /// Critical deadline task.
        const CRITICAL        = 1 << 5;
    }
}

/// Scheduling priority range.
pub const LOWEST_PRIORITY: i32 = 0;
/// Maximum scheduling priority.
pub const HIGHEST_PRIORITY: i32 = 31;

/// Profile configuration (matches `zx_profile_info_t` layout).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ProfileInfo {
    /// Bitmask of `ProfileInfoFlags`.
    pub flags_raw: u32,
    _padding1: u32,
    /// Scheduling priority (for PRIORITY or MEMORY_PRIORITY).
    pub priority: i32,
    _padding2: [u8; 20],
    /// CPU affinity mask (for CPU_MASK). 512 CPUs max.
    pub cpu_affinity_mask: [u64; 8],
}

impl ProfileInfo {
    /// Parse the flags field.
    pub fn flags(&self) -> ProfileInfoFlags {
        ProfileInfoFlags::from_bits_truncate(self.flags_raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_priority_profile() {
        let info = ProfileInfo {
            flags_raw: ProfileInfoFlags::PRIORITY.bits(),
            priority: 16,
            ..Default::default()
        };
        let profile = Profile::create(info).unwrap();
        assert_eq!(profile.info.priority, 16);
    }

    #[test]
    fn create_cpu_mask_only() {
        let mut info = ProfileInfo {
            flags_raw: ProfileInfoFlags::CPU_MASK.bits(),
            ..Default::default()
        };
        info.cpu_affinity_mask[0] = 0xF; // CPUs 0-3
        assert!(Profile::create(info).is_ok());
    }

    #[test]
    fn invalid_no_discipline() {
        // No flags at all -> invalid
        let info = ProfileInfo::default();
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_priority_and_deadline() {
        let info = ProfileInfo {
            flags_raw: (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::DEADLINE).bits(),
            priority: 10,
            ..Default::default()
        };
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_priority_out_of_range() {
        let info = ProfileInfo {
            flags_raw: ProfileInfoFlags::PRIORITY.bits(),
            priority: 100, // > HIGHEST_PRIORITY (31)
            ..Default::default()
        };
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_critical_without_deadline() {
        let info = ProfileInfo {
            flags_raw: (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CRITICAL).bits(),
            priority: 10,
            ..Default::default()
        };
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_no_inherit_with_deadline() {
        let info = ProfileInfo {
            flags_raw: (ProfileInfoFlags::DEADLINE | ProfileInfoFlags::NO_INHERIT).bits(),
            ..Default::default()
        };
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_memory_with_scheduling() {
        let info = ProfileInfo {
            flags_raw: (ProfileInfoFlags::MEMORY_PRIORITY | ProfileInfoFlags::PRIORITY).bits(),
            priority: 5,
            ..Default::default()
        };
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }
}
