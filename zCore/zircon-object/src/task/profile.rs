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
        let flags = ProfileInfoFlags::from_bits(info.flags_raw).ok_or(ZxError::INVALID_ARGS)?;
        let has_priority = flags.contains(ProfileInfoFlags::PRIORITY);
        let has_deadline = flags.contains(ProfileInfoFlags::DEADLINE);
        let has_memory = flags.contains(ProfileInfoFlags::MEMORY_PRIORITY);
        let has_critical = flags.contains(ProfileInfoFlags::CRITICAL);
        let has_no_inherit = flags.contains(ProfileInfoFlags::NO_INHERIT);

        let has_cpu_mask = flags.contains(ProfileInfoFlags::CPU_MASK);

        // Must specify exactly one scheduling discipline (or memory priority)
        if has_memory {
            // Memory priority is incompatible with all other flags
            if has_priority || has_deadline || has_cpu_mask {
                return Err(ZxError::INVALID_ARGS);
            }
            // Only ZX_PRIORITY_DEFAULT (16) and ZX_PRIORITY_HIGH (24) are valid
            let prio = info.priority();
            if prio != 16 && prio != 24 {
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
        if has_priority && !(LOWEST_PRIORITY..=HIGHEST_PRIORITY).contains(&info.priority()) {
            return Err(ZxError::INVALID_ARGS);
        }

        // Validate deadline parameters: 0 < capacity <= relative_deadline <= period
        if has_deadline {
            let dl = info.deadline_params();
            if dl.capacity <= 0
                || dl.relative_deadline <= 0
                || dl.period <= 0
                || dl.capacity > dl.relative_deadline
                || dl.relative_deadline > dl.period
            {
                return Err(ZxError::INVALID_ARGS);
            }
            // Reject out-of-range values (> INT32_MAX nanoseconds ≈ 2.1 seconds)
            // This matches the Fuchsia kernel's SchedDeadlineParams validation.
            const MAX_DEADLINE: i64 = i32::MAX as i64;
            if dl.capacity > MAX_DEADLINE
                || dl.relative_deadline > MAX_DEADLINE
                || dl.period > MAX_DEADLINE
            {
                return Err(ZxError::OUT_OF_RANGE);
            }
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

/// Deadline scheduling parameters (matches `zx_sched_deadline_params_t`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedDeadlineParams {
    /// Worst-case execution time per period (nanoseconds).
    pub capacity: i64,
    /// Worst-case finish time relative to period start (nanoseconds).
    pub relative_deadline: i64,
    /// Interarrival period (nanoseconds).
    pub period: i64,
}

/// Profile configuration (matches `zx_profile_info_t` layout).
///
/// The `priority` and `deadline` fields occupy the same memory (C union).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ProfileInfo {
    /// Bitmask of `ProfileInfoFlags`.
    pub flags_raw: u32,
    _padding1: u32,
    /// Union: priority (i32 + 20 bytes padding) or deadline params (24 bytes).
    sched_union: [u8; 24],
    /// CPU affinity mask (for CPU_MASK). 512 CPUs max.
    pub cpu_affinity_mask: [u64; 8],
}

impl core::fmt::Debug for ProfileInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProfileInfo")
            .field("flags_raw", &self.flags_raw)
            .field("priority", &self.priority())
            .finish()
    }
}

impl ProfileInfo {
    /// Parse the flags field.
    pub fn flags(&self) -> ProfileInfoFlags {
        ProfileInfoFlags::from_bits_truncate(self.flags_raw)
    }

    /// Read the priority field (first i32 of the union).
    pub fn priority(&self) -> i32 {
        i32::from_ne_bytes(self.sched_union[..4].try_into().unwrap())
    }

    /// Read priority (kept as a field-like accessor for compatibility).
    #[allow(non_snake_case)]
    pub fn get_priority(&self) -> i32 {
        self.priority()
    }

    /// Read the deadline parameters (entire 24-byte union).
    pub fn deadline_params(&self) -> SchedDeadlineParams {
        unsafe {
            core::ptr::read_unaligned(self.sched_union.as_ptr() as *const SchedDeadlineParams)
        }
    }

    /// Create a ProfileInfo with the given priority (for tests and internal use).
    pub fn with_priority(flags: u32, priority: i32) -> Self {
        let mut info = Self {
            flags_raw: flags,
            ..Default::default()
        };
        info.sched_union[..4].copy_from_slice(&priority.to_ne_bytes());
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_priority_profile() {
        let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
        let profile = Profile::create(info).unwrap();
        assert_eq!(profile.info.priority(), 16);
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
        let info = ProfileInfo::with_priority(
            (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::DEADLINE).bits(),
            10,
        );
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_priority_out_of_range() {
        let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 100);
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[test]
    fn invalid_critical_without_deadline() {
        let info = ProfileInfo::with_priority(
            (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CRITICAL).bits(),
            10,
        );
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
        let info = ProfileInfo::with_priority(
            (ProfileInfoFlags::MEMORY_PRIORITY | ProfileInfoFlags::PRIORITY).bits(),
            5,
        );
        assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
    }
}
