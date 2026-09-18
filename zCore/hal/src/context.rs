//! Trap reason and user context field types.

use crate::{MMUFlags, VirtAddr};

/// For reading and writing fields in a user context.
#[derive(Debug)]
pub enum UserContextField {
    InstrPointer,
    StackPointer,
    ThreadPointer,
    ReturnValue,
}

/// Reason of a trap from user space.
#[derive(Debug, PartialEq, Eq)]
pub enum TrapReason {
    Syscall,
    Interrupt(usize),
    PageFault(VirtAddr, MMUFlags),
    UndefinedInstruction,
    SoftwareBreakpoint,
    HardwareBreakpoint,
    UnalignedAccess,
    GeneralFault(usize),
}
