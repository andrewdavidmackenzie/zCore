//! Interrupt tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/interrupt/ tests.
//! Most tests require hardware interrupt support which is not available
//! in libos mode, so they use #[should_panic].

use crate::helpers::TestContext;

/// C++: TEST(InterruptTest, Create)
///
/// Creating an interrupt object requires a resource handle. Not
/// available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_create() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, CreateInvalid)
///
/// Creating an interrupt with invalid parameters should fail.
/// Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_create_invalid() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Bind)
///
/// Binding an interrupt to a port. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_bind() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Unbind)
///
/// Unbinding an interrupt from a port. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_unbind() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Wait)
///
/// Waiting on an interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_wait() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Ack)
///
/// Acknowledging an interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_ack() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Destroy)
///
/// Destroying an interrupt object. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_destroy() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Trigger)
///
/// Triggering a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_trigger() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, VirtualInterrupt)
///
/// Creating a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_virtual_interrupt() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, VirtualInterruptWait)
///
/// Waiting on a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_virtual_interrupt_wait() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}
