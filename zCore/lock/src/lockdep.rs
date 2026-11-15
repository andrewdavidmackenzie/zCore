//! Lock ordering enforcement (lockdep).
//!
//! Tracks which lock levels each CPU currently holds and asserts that
//! new lock acquisitions respect the canonical ordering. Violations
//! are detected at runtime via `debug_assert!`.
//!
//! # Canonical lock ordering (highest → lowest)
//!
//! ```text
//! Level 6: Pager::inner             (L6)
//! Level 5: VmAddressRegion::inner   (L5)
//! Level 4: VMObjectPaged::lock      (L2, clone-tree YieldingMutex)
//! Level 3: VmObject::inner          (L1)
//! Level 2: VmMapping::inner         (L3)
//! Level 1: page_table               (L4)
//! Level 0: unordered — no checking  (Port, Futex, KObjectBase, etc.)
//! ```
//!
//! A lock at level N may only be acquired while holding locks at
//! levels > N (or no locks). Acquiring a lock at level ≤ any
//! currently held level is a violation.
//!
//! Level 0 is a special "unordered" level that bypasses all checks.
//! Use it for locks that don't participate in the VM lock hierarchy
//! (Port, Futex, Thread, Process, etc.).

/// Lock ordering levels. Higher numbers must be acquired before
/// lower numbers. Level 0 means "unordered" — no checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LockLevel {
    /// No ordering enforcement. Use for locks outside the VM hierarchy.
    Unordered = 0,
    /// page_table (L4 in the audit). Lowest ordered level.
    PageTable = 1,
    /// VmMapping::inner (L3 in the audit).
    VmMappingInner = 2,
    /// VmObject::inner (L1 in the audit).
    VmObjectInner = 3,
    /// VMObjectPaged::lock (L2 in the audit, YieldingMutex).
    VmObjectPaged = 4,
    /// VmAddressRegion::inner (L5 in the audit).
    VmarInner = 5,
    /// Pager::inner (L6 in the audit).
    PagerInner = 6,
}

// ── Per-CPU held-level tracking ──────────────────────────────────────
//
// On bare-metal, we use a fixed array indexed by CPU ID.
// On hosted (libos), we use a thread-local.

#[cfg(target_os = "none")]
mod per_cpu {
    use core::sync::atomic::{AtomicU8, Ordering};

    /// Maximum CPUs supported. Matches the 8-CPU QEMU config.
    const MAX_CPUS: usize = 64;

    /// Per-CPU bitmask of currently held lock levels (bits 1-6).
    /// Bit N set means a lock at level N is currently held.
    static HELD_LEVELS: [AtomicU8; MAX_CPUS] = {
        const INIT: AtomicU8 = AtomicU8::new(0);
        [INIT; MAX_CPUS]
    };

    /// Get the CPU ID. Falls back to 0 if HAL isn't initialized yet.
    #[inline]
    fn cpu_id() -> usize {
        // Use the HAL's cpu_id if available. During early boot before
        // the HAL is initialized, default to 0 (BSP).
        #[cfg(target_arch = "x86_64")]
        {
            // Read LAPIC ID from the IA32_TSC_AUX MSR (set by the kernel
            // to the CPU index). Falls back to 0 on error.
            let id: u32;
            unsafe {
                core::arch::asm!(
                    "rdtscp",
                    out("ecx") id,
                    out("eax") _,
                    out("edx") _,
                    options(nomem, nostack),
                );
            }
            (id as usize).min(MAX_CPUS - 1)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            0 // Conservative fallback for non-x86
        }
    }

    #[inline]
    pub(super) fn held_levels() -> u8 {
        HELD_LEVELS[cpu_id()].load(Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn add_level(level: u8) {
        let id = cpu_id();
        let old = HELD_LEVELS[id].load(Ordering::Relaxed);
        HELD_LEVELS[id].store(old | (1 << level), Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn remove_level(level: u8) {
        let id = cpu_id();
        let old = HELD_LEVELS[id].load(Ordering::Relaxed);
        HELD_LEVELS[id].store(old & !(1 << level), Ordering::Relaxed);
    }
}

#[cfg(not(target_os = "none"))]
mod per_cpu {
    // In libos/hosted mode, use a simple global (single-threaded tests).
    // A proper implementation would use thread-local storage.
    static HELD: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

    #[inline]
    pub(super) fn held_levels() -> u8 {
        HELD.load(core::sync::atomic::Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn add_level(level: u8) {
        let old = HELD.load(core::sync::atomic::Ordering::Relaxed);
        HELD.store(old | (1 << level), core::sync::atomic::Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn remove_level(level: u8) {
        let old = HELD.load(core::sync::atomic::Ordering::Relaxed);
        HELD.store(old & !(1 << level), core::sync::atomic::Ordering::Relaxed);
    }
}

/// Return the current held-level bitmask for this CPU (for diagnostics).
#[inline]
pub fn held_levels() -> u8 {
    per_cpu::held_levels()
}

/// Check that acquiring a lock at `level` doesn't violate ordering.
///
/// Panics (in debug builds) if any currently held lock has a level
/// ≤ the requested level. This catches ABBA deadlocks at the point
/// of the second acquisition.
#[inline]
pub fn check_lock_order(level: LockLevel) {
    let lvl = level as u8;
    if lvl == 0 {
        return; // Unordered locks skip all checks
    }
    let held = per_cpu::held_levels();
    // Check: no held lock at level ≤ lvl. Held levels are in bits 1-6.
    // Mask out bits 0..=lvl to find violations.
    let conflict_mask = held & ((1u8 << (lvl + 1)) - 1);
    // Bit 0 is always unused (level 0 = unordered, never tracked).
    // Only check bits 1..=lvl.
    let conflict = conflict_mask & !1; // clear bit 0
    debug_assert!(
        conflict == 0,
        "lockdep: lock ordering violation! Acquiring level {:?} (level {}) \
         but already holding levels with mask {:#04x}. \
         Held levels (bitmask): {:#04x}",
        level,
        lvl,
        conflict,
        held,
    );
}

/// Record that a lock at `level` has been acquired.
#[inline]
pub fn on_lock_acquired(level: LockLevel) {
    let lvl = level as u8;
    if lvl == 0 {
        return;
    }
    per_cpu::add_level(lvl);
}

/// Record that a lock at `level` has been released.
#[inline]
pub fn on_lock_released(level: LockLevel) {
    let lvl = level as u8;
    if lvl == 0 {
        return;
    }
    per_cpu::remove_level(lvl);
}
