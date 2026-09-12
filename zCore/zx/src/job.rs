//! Zircon job wrapper.

use crate::{impl_handle_based, Handle};

/// A Zircon job object.
#[derive(Debug)]
pub struct Job(Handle);
impl_handle_based!(Job);
