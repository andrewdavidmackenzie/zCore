//! Process init info

use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::{align_of, size_of};
use core::ops::Deref;
use core::ptr::null;

/// process init information
pub struct ProcInitInfo {
    /// args strings
    pub args: Vec<String>,
    /// environment strings
    pub envs: Vec<String>,
    /// auxiliary
    pub auxv: BTreeMap<u8, usize>,
}

impl ProcInitInfo {
    /// push process init information into stack
    pub fn push_at(&self, stack_top: usize) -> Stack {
        let mut writer = Stack::new(stack_top);
        // from stack_top:
        // program name
        writer.push_str(&self.args[0]);
        // 16 random bytes for AT_RANDOM (musl uses this for stack canary / TLS)
        let mut random_bytes = [0u8; 16];
        kernel_hal::rand::fill_random(&mut random_bytes);
        writer.push_slice(&random_bytes);
        let random_ptr = writer.sp;
        // environment strings
        let envs: Vec<_> = self
            .envs
            .iter()
            .map(|arg| {
                writer.push_str(arg.as_str());
                writer.sp
            })
            .collect();
        // argv strings
        let argv: Vec<_> = self
            .args
            .iter()
            .map(|arg| {
                writer.push_str(arg.as_str());
                writer.sp
            })
            .collect();
        // auxiliary vector entries
        writer.push_slice(&[null::<u8>(), null::<u8>()]);
        for (&type_, &value) in self.auxv.iter() {
            // AT_RANDOM is handled specially with the stack pointer
            if type_ == AT_RANDOM {
                writer.push_slice(&[type_ as usize, random_ptr]);
            } else {
                writer.push_slice(&[type_ as usize, value]);
            }
        }
        // envionment pointers
        writer.push_slice(&[null::<u8>()]);
        writer.push_slice(envs.as_slice());
        // argv pointers
        writer.push_slice(&[null::<u8>()]);
        writer.push_slice(argv.as_slice());
        // argc
        writer.push_slice(&[argv.len()]);
        writer
    }
}

/// program stack
pub struct Stack {
    /// stack pointer
    sp: usize,
    /// stack top
    stack_top: usize,
    /// stack data buffer
    data: Vec<u8>,
}

impl Stack {
    /// create a stack
    fn new(sp: usize) -> Self {
        let data = vec![0u8; 0x4000];
        Stack {
            sp,
            stack_top: sp,
            data,
        }
    }
    /// push slice into stack
    #[allow(unsafe_code)]
    fn push_slice<T: Copy>(&mut self, vs: &[T]) {
        self.sp -= vs.len() * size_of::<T>();
        self.sp -= self.sp % align_of::<T>();
        assert!(self.stack_top - self.sp <= self.data.len());
        let offset = self.data.len() - (self.stack_top - self.sp);
        unsafe {
            core::slice::from_raw_parts_mut(self.data.as_mut_ptr().add(offset) as *mut T, vs.len())
        }
        .copy_from_slice(vs);
    }
    /// push str into stack
    fn push_str(&mut self, s: &str) {
        self.push_slice(&[b'\0']);
        self.push_slice(s.as_bytes());
    }
}

impl Deref for Stack {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let offset = self.data.len() - (self.stack_top - self.sp);
        &self.data[offset..]
    }
}

pub const AT_PHDR: u8 = 3;
pub const AT_PHENT: u8 = 4;
pub const AT_PHNUM: u8 = 5;
pub const AT_PAGESZ: u8 = 6;
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub const AT_BASE: u8 = 7;
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub const AT_ENTRY: u8 = 9;
/// Real user ID of the process.
pub const AT_UID: u8 = 11;
/// Effective user ID of the process.
pub const AT_EUID: u8 = 12;
/// Real group ID of the process.
pub const AT_GID: u8 = 13;
/// Effective group ID of the process.
pub const AT_EGID: u8 = 14;
/// Frequency at which times() counts (typically 100 on Linux).
pub const AT_CLKTCK: u8 = 17;
/// Boolean: true if running in secure mode (setuid/setgid).
pub const AT_SECURE: u8 = 23;
/// Pointer to 16 random bytes provided by the kernel.
/// Used by musl for stack canaries and TLS initialization.
pub const AT_RANDOM: u8 = 25;
/// CPU hardware capabilities bitmask.
pub const AT_HWCAP: u8 = 16;
