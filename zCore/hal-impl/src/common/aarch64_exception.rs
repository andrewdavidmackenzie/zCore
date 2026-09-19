//! AArch64 exception model types.
//!
//! Types for decoding ARMv8-A exception information from the
//! Exception Syndrome Register (ESR_EL1). Compiled only on aarch64.
//!
//! These are used by `TrapReason::from()` in `context.rs` and by the
//! bare-metal trap handler in `bare/arch/aarch64/trap.rs`.

/// The kind of exception.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Kind {
    Synchronous = 0,
    Irq = 1,
    Fiq = 2,
    SError = 3,
}

impl Kind {
    pub fn from(x: usize) -> Kind {
        match x {
            x if x == Kind::Synchronous as usize => Kind::Synchronous,
            x if x == Kind::Irq as usize => Kind::Irq,
            x if x == Kind::Fiq as usize => Kind::Fiq,
            x if x == Kind::SError as usize => Kind::SError,
            _ => panic!("bad kind"),
        }
    }
}

/// Where the exception was taken from.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Source {
    CurrentSpEl0 = 0,
    CurrentSpElx = 1,
    LowerAArch64 = 2,
    LowerAArch32 = 3,
}

impl Source {
    pub fn from(x: usize) -> Source {
        match x {
            x if x == Source::CurrentSpEl0 as usize => Source::CurrentSpEl0,
            x if x == Source::CurrentSpElx as usize => Source::CurrentSpElx,
            x if x == Source::LowerAArch64 as usize => Source::LowerAArch64,
            x if x == Source::LowerAArch32 as usize => Source::LowerAArch32,
            _ => panic!("bad kind"),
        }
    }
}

/// Exception source and kind combined.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Info {
    pub source: Source,
    pub kind: Kind,
}

/// Fault type decoded from IFSC/DFSC fields of ESR.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Fault {
    AddressSize,
    Translation,
    AccessFlag,
    Permission,
    Alignment,
    TlbConflict,
    Other(u8),
}

impl From<u32> for Fault {
    fn from(val: u32) -> Fault {
        use self::Fault::*;

        // IFSC or DFSC bits (ref: D10.2.39, Page 2457~2464).
        match val & 0b111100 {
            0b000000 => AddressSize,
            0b000100 => Translation,
            0b001000 => AccessFlag,
            0b001100 => Permission,
            0b100000 => Alignment,
            0b110000 => TlbConflict,
            _ => Other((val & 0b111111) as u8),
        }
    }
}

/// Exception syndrome decoded from ESR_EL1.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Syndrome {
    Unknown,
    WfiWfe,
    McrMrc,
    McrrMrrc,
    LdcStc,
    SimdFp,
    Vmrs,
    Mrrc,
    IllegalExecutionState,
    Svc(u16),
    Hvc(u16),
    Smc(u16),
    MsrMrsSystem,
    InstructionAbort {
        kind: Fault,
        level: u8,
    },
    PCAlignmentFault,
    DataAbort {
        kind: Fault,
        level: u8,
        /// WnR bit (ISS bit 6): true if the fault was caused by a write access.
        is_write: bool,
    },
    SpAlignmentFault,
    TrappedFpu,
    SError,
    Breakpoint,
    Step,
    Watchpoint,
    Brk(u16),
    Other(u32),
}

/// Converts a raw syndrome value (ESR) into a `Syndrome` (ref: D1.10.4, D10.2.39).
impl From<u32> for Syndrome {
    fn from(esr: u32) -> Syndrome {
        use self::Syndrome::*;

        let ec = esr >> 26;
        let iss = esr & 0xFFFFFF;

        match ec {
            0b000000 => Unknown,
            0b000001 => WfiWfe,
            0b000011 => McrMrc,
            0b000100 => McrrMrrc,
            0b000101 => McrMrc,
            0b000110 => LdcStc,
            0b000111 => SimdFp,
            0b001000 => Vmrs,
            0b001100 => Mrrc,
            0b001110 => IllegalExecutionState,
            0b010001 => Svc((iss & 0xFFFF) as u16),
            0b010010 => Hvc((iss & 0xFFFF) as u16),
            0b010011 => Smc((iss & 0xFFFF) as u16),
            0b010101 => Svc((iss & 0xFFFF) as u16),
            0b010110 => Hvc((iss & 0xFFFF) as u16),
            0b010111 => Smc((iss & 0xFFFF) as u16),
            0b011000 => MsrMrsSystem,
            0b100000 | 0b100001 => InstructionAbort {
                kind: Fault::from(iss),
                level: (iss & 0b11) as u8,
            },
            0b100010 => PCAlignmentFault,
            0b100100 | 0b100101 => DataAbort {
                kind: Fault::from(iss),
                level: (iss & 0b11) as u8,
                is_write: iss & (1 << 6) != 0,
            },
            0b100110 => SpAlignmentFault,
            0b101000 => TrappedFpu,
            0b101100 => TrappedFpu,
            0b101111 => SError,
            0b110000 => Breakpoint,
            0b110001 => Breakpoint,
            0b110010 => Step,
            0b110011 => Step,
            0b110100 => Watchpoint,
            0b110101 => Watchpoint,
            0b111100 => Brk((iss & 0xFFFF) as u16),
            other => Other(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an ESR value from exception class and ISS fields.
    fn make_esr(ec: u32, iss: u32) -> u32 {
        (ec << 26) | (iss & 0xFF_FFFF)
    }

    #[test]
    fn data_abort_read_clears_is_write() {
        // EC 0b100100 = DataAbort from lower EL.
        // ISS: Translation fault level 1 (DFSC=0b000101), WnR=0 (bit 6 clear).
        let esr = make_esr(0b100100, 0b000101);
        match Syndrome::from(esr) {
            Syndrome::DataAbort {
                kind,
                level,
                is_write,
            } => {
                assert_eq!(kind, Fault::Translation);
                assert_eq!(level, 1);
                assert!(!is_write, "WnR should be clear for a read fault");
            }
            other => panic!("expected DataAbort, got {:?}", other),
        }
    }

    #[test]
    fn data_abort_write_sets_is_write() {
        // EC 0b100101 = DataAbort from current EL.
        // ISS: Permission fault level 3 (DFSC=0b001111), WnR=1 (bit 6 set).
        let iss = 0b001111 | (1 << 6);
        let esr = make_esr(0b100101, iss);
        match Syndrome::from(esr) {
            Syndrome::DataAbort {
                kind,
                level,
                is_write,
            } => {
                assert_eq!(kind, Fault::Permission);
                assert_eq!(level, 3);
                assert!(is_write, "WnR should be set for a write fault");
            }
            other => panic!("expected DataAbort, got {:?}", other),
        }
    }

    #[test]
    fn instruction_abort_has_no_is_write() {
        // EC 0b100000 = InstructionAbort from lower EL.
        // ISS: Translation fault level 2 (IFSC=0b000110).
        let esr = make_esr(0b100000, 0b000110);
        match Syndrome::from(esr) {
            Syndrome::InstructionAbort { kind, level } => {
                assert_eq!(kind, Fault::Translation);
                assert_eq!(level, 2);
            }
            other => panic!("expected InstructionAbort, got {:?}", other),
        }
    }
}
