//! Executable TLS tests.
//!
//! Ports of Fuchsia's executable TLS test suite. Tests thread-local
//! storage initialization and access patterns in ELF executables.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ExecutableTlsTest, AlignmentInitializerInThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn alignment_initializer_in_thread() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, AlignmentInitializierInMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn alignment_initializier_in_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn array_initializer_in_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn array_initializer_in_thread() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerSpamThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn array_initializer_spam_thread() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializierSpamMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn array_initializier_spam_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn basic_initalizers_in_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn basic_initalizers_in_thread() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn big_array_initializer_in_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn big_array_initializer_in_thread() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, StructureInitalizierInMain)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn structure_initalizier_in_main() {
    todo!("requires ELF TLS infrastructure");
}

/// C++: TEST(ExecutableTlsTest, StructureInitializerInThread)
#[test]
#[ignore = "requires ELF TLS infrastructure"]
fn structure_initializer_in_thread() {
    todo!("requires ELF TLS infrastructure");
}
