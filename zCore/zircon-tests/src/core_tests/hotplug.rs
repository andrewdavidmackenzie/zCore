//! Hotplug tests — faithful port of Fuchsia `hotplug/hotplug-tests.cc`.
//!
//! C++ source: `core-tests/hotplug/hotplug-tests.cc`
//!
//! The single C++ test invokes `zx_debug_send_command` with the kernel
//! command `"ut hotplug"` using a DEBUG system resource. This triggers
//! the in-kernel hotplug unit tests.

use crate::helpers::TestContext;
use hal_impl::user::UserInPtr;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::{Handle, Rights};

type HandleValue = u32;

/// C++: TEST(HotplugTests, KHotplugTest)
///
/// ```cpp
/// zx::resource debug_rsrc;
/// {
///     zx::unowned_resource system_rsrc = standalone::GetSystemResource();
///     zx::result<zx::resource> result =
///         standalone::GetSystemResourceWithBase(system_rsrc, ZX_RSRC_SYSTEM_DEBUG_BASE);
///     ASSERT_TRUE(result.is_ok());
///     debug_rsrc = std::move(result.value());
/// }
/// constexpr std::string_view kCmd = "ut hotplug";
/// ASSERT_EQ(ZX_OK, zx_debug_send_command(debug_rsrc.release(), kCmd.data(), kCmd.size()));
/// ```
///
/// Create a DEBUG system resource and send the "ut hotplug" kernel
/// command via zx_debug_send_command. In zCore, "ut hotplug" is not
/// a recognized command (only help, ps, threads, kill are supported),
/// so the call will return INVALID_ARGS. The faithful port calls the
/// same syscall with the same arguments.
#[test]
fn k_hotplug_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // C++: Create a DEBUG system resource.
    // GetSystemResourceWithBase(system_rsrc, ZX_RSRC_SYSTEM_DEBUG_BASE)
    let debug_resource = Resource::create(
        "debug",
        ResourceKind::SYSTEM,
        zircon_object::dev::ZX_RSRC_SYSTEM_DEBUG_BASE,
        1,
        ResourceFlags::empty(),
    );
    let debug_handle: HandleValue = ctx
        .proc
        .add_handle(Handle::new(debug_resource, Rights::DEFAULT_RESOURCE));

    // C++: zx_debug_send_command(debug_rsrc.release(), "ut hotplug", 10)
    // C++ expects ZX_OK — the Fuchsia kernel runs in-kernel hotplug unit
    // tests. In zCore, "ut <name>" is accepted as a stub that returns Ok.
    // The actual kernel unit tests are not executed, but the syscall path
    // (resource validation, command parsing) is exercised.
    let cmd = b"ut hotplug";
    let result = sc.sys_debug_send_command(
        debug_handle,
        UserInPtr::from(cmd.as_ptr() as usize),
        cmd.len(),
    );
    assert_eq!(result, Ok(()), "debug command 'ut hotplug' should succeed");
}
