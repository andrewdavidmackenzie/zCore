//! Zircon thread wrapper.

use crate::{impl_handle_based, Handle};

/// A Zircon thread object.
#[derive(Debug)]
pub struct Thread(Handle);
impl_handle_based!(Thread);
