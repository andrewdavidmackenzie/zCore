//! ZX assert tests.
//!
//! Ports of Fuchsia's zx_assert macro test suite.
//! zCore has no zx_assert macro. These tests verify that
//! Rust assert macros work correctly with kernel error types.

use crate::helpers::TestContext;
use zircon_object::ZxError;
