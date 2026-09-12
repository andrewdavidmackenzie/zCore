//! Safe Rust bindings for Zircon kernel objects.
//!
//! This crate provides idiomatic, type-safe wrappers around the raw
//! Zircon syscall interface defined in [`zircon-abi`]. Handles are
//! automatically closed on drop, errors are returned as `Result`, and
//! each kernel object type gets its own Rust type.
//!
//! # Example
//! ```no_run
//! use zx::{Channel, Status};
//!
//! let (ch0, ch1) = Channel::create()?;
//! ch0.write(b"hello", &[])?;
//! let mut buf = [0u8; 64];
//! let (bytes, _handles) = ch1.read(&mut buf, &mut [])?;
//! # Ok::<(), Status>(())
//! ```
//!
//! # Re-exports
//! The raw syscall bindings are available as `zx::sys`:
//! ```
//! use zx::sys; // re-exports zircon_abi
//! ```

#![no_std]
#![deny(warnings)]

/// Raw syscall bindings re-exported from `zircon-abi`.
pub mod sys {
    pub use zircon_abi::consts::*;
    pub use zircon_abi::errors::*;
    pub use zircon_abi::syscall::*;
    // Re-export types, excluding HandleValue which is already in syscall.
    pub use zircon_abi::types::{
        ChannelCallArgs, ExceptionContext, ExceptionHeader, ExceptionReport, HandleBasicInfo,
        HandleDisposition, HandleInfo, PortPacket, ProcessInfo, ThreadInfo, WaitItem,
    };
}

mod status;
pub use status::Status;

mod handle;
pub use handle::{Handle, HandleBased, HandleRef};

mod channel;
pub use channel::Channel;

mod vmo;
pub use vmo::Vmo;

mod event;
pub use event::{Event, EventPair};

mod port;
pub use port::Port;

mod timer;
pub use timer::Timer;

mod debuglog;
pub use debuglog::{debug_write, DebugLog};

mod process;
pub use process::{process_self, Process};

mod thread;
pub use thread::Thread;

mod job;
pub use job::Job;

mod socket;
pub use socket::Socket;
