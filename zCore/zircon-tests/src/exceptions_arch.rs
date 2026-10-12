//! Architecture-specific exception tests.
//!
//! Ports of Fuchsia's architecture-specific exception test suite.
//! Tests ARM64 exception types: data abort, instruction abort, etc.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ExceptionsTest, DataAbort)
#[test]
#[ignore = "requires architecture-specific exception testing"]
fn data_abort() {
    todo!("requires architecture-specific exception testing");
}

/// C++: TEST(ExceptionsTest, InstructionAbort)
#[test]
#[ignore = "requires architecture-specific exception testing"]
fn instruction_abort() {
    todo!("requires architecture-specific exception testing");
}

/// C++: TEST(ExceptionsTest, PCAlignmentFault)
#[test]
#[ignore = "requires architecture-specific exception testing"]
fn pc_alignment_fault() {
    todo!("requires architecture-specific exception testing");
}

/// C++: TEST(ExceptionsTest, SPMisalignment)
#[test]
#[ignore = "requires architecture-specific exception testing"]
fn sp_misalignment() {
    todo!("requires architecture-specific exception testing");
}
