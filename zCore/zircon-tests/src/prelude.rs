//! Common imports for no_std compatibility.
//!
//! Re-exports alloc types that are in the std prelude but not
//! available in no_std mode.

pub use alloc::boxed::Box;
pub use alloc::string::String;
pub use alloc::string::ToString;
pub use alloc::vec::Vec;
