//! System CPU tests — faithful port of Fuchsia `system-cpu/performance_info.cc`.
//!
//! C++ source: `core-tests/system-cpu/performance_info.cc`
//!
//! The C++ tests exercise `zx_system_get_performance_info` and
//! `zx_system_set_performance_info` with various argument validation
//! patterns. We call the same syscalls through the zCore dispatch layer.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::{Handle, Rights};
use zircon_object::ZxError;

type HandleValue = u32;

// Topic constants matching Fuchsia's zircon/syscalls/system.h
const ZX_CPU_PERF_SCALE: u32 = 1;
#[allow(dead_code)]
const ZX_CPU_DEFAULT_PERF_SCALE: u32 = 2;
#[allow(dead_code)]
const ZX_CPU_PERF_LIMIT: u32 = 3;

/// Helper: create a SYSTEM resource with CPU_BASE range and add to process.
fn add_cpu_resource(ctx: &TestContext) -> HandleValue {
    let resource = Resource::create(
        "system-cpu",
        ResourceKind::SYSTEM,
        zircon_object::dev::ZX_RSRC_SYSTEM_CPU_BASE,
        1,
        ResourceFlags::empty(),
    );
    ctx.proc
        .add_handle(Handle::new(resource, Rights::DEFAULT_RESOURCE))
}

/// Helper: create a SYSTEM resource with INFO_BASE range.
fn add_info_resource(ctx: &TestContext) -> HandleValue {
    let resource = Resource::create(
        "system-info",
        ResourceKind::SYSTEM,
        zircon_object::dev::ZX_RSRC_SYSTEM_INFO_BASE,
        1,
        ResourceFlags::empty(),
    );
    ctx.proc
        .add_handle(Handle::new(resource, Rights::DEFAULT_RESOURCE))
}

/// Helper: create an MMIO resource (wrong type for CPU operations).
fn add_mmio_resource(ctx: &TestContext) -> HandleValue {
    let resource = Resource::create(
        "mmio",
        ResourceKind::MMIO,
        0,
        0x1000,
        ResourceFlags::empty(),
    );
    ctx.proc
        .add_handle(Handle::new(resource, Rights::DEFAULT_RESOURCE))
}

/// C++: TEST(SystemCpu, SetPerformanceInfoArgumentValidation)
///
/// Validates argument checking for `zx_system_set_performance_info`.
/// Tests: BAD_HANDLE, WRONG_TYPE, OUT_OF_RANGE, INVALID_ARGS, and
/// valid cases.
#[test]
fn set_performance_info_argument_validation() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let cpu_resource = add_cpu_resource(&ctx);

    // C++: invalid handle → ZX_ERR_BAD_HANDLE
    {
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]; // cpu=0, scale={1,0}
        let result = sc.sys_system_set_performance_info(
            0, // ZX_HANDLE_INVALID
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Err(ZxError::BAD_HANDLE));
    }

    // C++: incorrect resource kind → ZX_ERR_WRONG_TYPE
    {
        let mmio_resource = add_mmio_resource(&ctx);
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            mmio_resource,
            u32::MAX, // kInvalidTopic
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Err(ZxError::WRONG_TYPE));
    }

    // C++: incorrect system resource range → ZX_ERR_OUT_OF_RANGE
    {
        let info_resource = add_info_resource(&ctx);
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            info_resource,
            u32::MAX, // kInvalidTopic
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: invalid topic → ZX_ERR_INVALID_ARGS
    {
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            u32::MAX, // kInvalidTopic
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Err(ZxError::INVALID_ARGS));
    }

    // C++: invalid info pointer (nullptr) → ZX_ERR_INVALID_ARGS
    {
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            0, // nullptr
            1,
        );
        assert_eq!(result, Err(ZxError::INVALID_ARGS));
    }

    // C++: count == 0 → ZX_ERR_OUT_OF_RANGE
    {
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            0,
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: info_count > num_cpus → ZX_ERR_OUT_OF_RANGE
    {
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            usize::MAX, // kInvalidInfoCount
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: invalid perf scale (0.0) → ZX_ERR_OUT_OF_RANGE
    {
        // cpu=0, scale={integral_part=0, fractional_part=0} → zero scale
        // 12 bytes: u32 cpu + u32 integral + u32 fractional
        let info: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: clamped perf scale {1, 1} → ZX_OK
    {
        // cpu=0, scale={integral_part=1, fractional_part=1}
        // [cpu:0,0,0,0] [integral:1,0,0,0] [fractional:1,0,0,0]
        let info: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Ok(()));
    }

    // C++: clamped perf scale {2, 0} → ZX_OK
    {
        // cpu=0, scale={integral_part=2, fractional_part=0}
        let info: [u8; 12] = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];
        let result = sc.sys_system_set_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            info.as_ptr() as usize,
            1,
        );
        assert_eq!(result, Ok(()));
    }
}

/// C++: TEST(SystemCpu, GetPerformanceInfoArgumentValidation)
///
/// Validates argument checking for `zx_system_get_performance_info`.
#[test]
fn get_performance_info_argument_validation() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let cpu_resource = add_cpu_resource(&ctx);

    let online_cpus: usize = 1; // zCore reports 1 CPU

    // C++: invalid handle → ZX_ERR_BAD_HANDLE
    {
        let mut perf_info = [0u8; 12]; // one entry
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            0, // ZX_HANDLE_INVALID
            u32::MAX,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::BAD_HANDLE));
    }

    // C++: incorrect resource kind → ZX_ERR_WRONG_TYPE
    {
        let mmio_resource = add_mmio_resource(&ctx);
        let mut perf_info = [0u8; 12];
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            mmio_resource,
            u32::MAX,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::WRONG_TYPE));
    }

    // C++: incorrect system resource range → ZX_ERR_OUT_OF_RANGE
    {
        let info_resource = add_info_resource(&ctx);
        let mut perf_info = [0u8; 12];
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            info_resource,
            u32::MAX,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: invalid topic → ZX_ERR_INVALID_ARGS
    {
        let mut perf_info = [0u8; 12];
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            u32::MAX,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::INVALID_ARGS));
    }

    // C++: info_count == 0 → ZX_ERR_OUT_OF_RANGE (for both PERF_SCALE and PERF_LIMIT)
    {
        let mut perf_info = [0u8; 12];
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            0,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: info_count > num_cpus → ZX_ERR_OUT_OF_RANGE
    {
        let mut perf_info = [0u8; 16]; // 2 entries
        let mut count: usize = 0;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            online_cpus + 1,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Err(ZxError::OUT_OF_RANGE));
    }

    // C++: info_count < num_cpus → ZX_ERR_OUT_OF_RANGE
    // (only applicable when num_cpus > 1, skip for single-CPU)

    // C++: invalid output_count (nullptr) → ZX_ERR_INVALID_ARGS
    {
        let mut perf_info = [0u8; 12];
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(0usize), // nullptr
        );
        assert_eq!(result, Err(ZxError::INVALID_ARGS));
    }
}

/// C++: TEST(SystemCpu, GetPerformanceInfo)
///
/// Successfully queries default perf scale, current perf scale, and
/// perf limits. Verifies entries are sorted by CPU number and have
/// non-zero scale values.
#[test]
fn get_performance_info() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let cpu_resource = add_cpu_resource(&ctx);

    let online_cpus: usize = 1; // zCore reports 1 CPU

    // Query ZX_CPU_DEFAULT_PERF_SCALE (topic 2)
    {
        let mut perf_info = [0u8; 12]; // 1 entry: { u32 cpu, u32 integral, u32 fractional }
        let mut count: usize = usize::MAX;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_DEFAULT_PERF_SCALE,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(count, online_cpus);

        // Parse: cpu_number (u32) + scale { u32 integral, u32 fractional }
        let cpu = u32::from_ne_bytes([perf_info[0], perf_info[1], perf_info[2], perf_info[3]]);
        let integral = u32::from_ne_bytes([perf_info[4], perf_info[5], perf_info[6], perf_info[7]]);
        let fractional =
            u32::from_ne_bytes([perf_info[8], perf_info[9], perf_info[10], perf_info[11]]);
        assert_eq!(cpu, 0);
        // C++: EXPECT_FALSE(integral == 0 && fractional == 0)
        assert!(
            !(integral == 0 && fractional == 0),
            "default perf scale should not be zero"
        );
    }

    // Query ZX_CPU_PERF_SCALE (topic 1)
    {
        let mut perf_info = [0u8; 12];
        let mut count: usize = usize::MAX;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_PERF_SCALE,
            online_cpus,
            perf_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(count, online_cpus);

        let cpu = u32::from_ne_bytes([perf_info[0], perf_info[1], perf_info[2], perf_info[3]]);
        let integral = u32::from_ne_bytes([perf_info[4], perf_info[5], perf_info[6], perf_info[7]]);
        let fractional =
            u32::from_ne_bytes([perf_info[8], perf_info[9], perf_info[10], perf_info[11]]);
        assert_eq!(cpu, 0);
        assert!(
            !(integral == 0 && fractional == 0),
            "current perf scale should not be zero"
        );
    }

    // Query ZX_CPU_PERF_LIMIT (topic 3)
    {
        // zx_cpu_perf_limit_t = { u32 cpu, u32 limit_type, u64 min, u64 max } = 24 bytes
        let mut limit_info = [0u8; 24];
        let mut count: usize = usize::MAX;
        let result = sc.sys_system_get_performance_info(
            cpu_resource,
            ZX_CPU_PERF_LIMIT,
            online_cpus,
            limit_info.as_mut_ptr() as usize,
            UserOutPtr::from(&mut count as *mut _ as usize),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(count, online_cpus);

        let cpu = u32::from_ne_bytes([limit_info[0], limit_info[1], limit_info[2], limit_info[3]]);
        let limit_type =
            u32::from_ne_bytes([limit_info[4], limit_info[5], limit_info[6], limit_info[7]]);
        let min = u64::from_ne_bytes([
            limit_info[8],
            limit_info[9],
            limit_info[10],
            limit_info[11],
            limit_info[12],
            limit_info[13],
            limit_info[14],
            limit_info[15],
        ]);
        let max = u64::from_ne_bytes([
            limit_info[16],
            limit_info[17],
            limit_info[18],
            limit_info[19],
            limit_info[20],
            limit_info[21],
            limit_info[22],
            limit_info[23],
        ]);

        assert_eq!(cpu, 0);
        // C++: EXPECT_EQ(entry.limit_type, ZX_CPU_PERF_LIMIT_TYPE_RATE)
        assert_eq!(limit_type, 0); // ZX_CPU_PERF_LIMIT_TYPE_RATE = 0
                                   // C++: EXPECT_EQ(entry.min, 0)
        assert_eq!(min, 0);
        // C++: EXPECT_EQ(entry.max, 1000)
        assert_eq!(max, 1000);
    }
}

/// C++: TEST(SystemCpu, TargetPreemptionTimeAssert)
///
/// Verify that the scheduler's target preemption time is properly updated
/// when the performance scale changes. This test sets perf scale to 1.0,
/// then 0.5, and spins.
#[test]
fn target_preemption_time_assert() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let cpu_resource = add_cpu_resource(&ctx);

    // C++: if (GetCpuCount() < kTestThreadCpu + 1) { return; }
    // zCore has 1 CPU, kTestThreadCpu=1 → 1 < 2 → skip.
    // But we still exercise the set_performance_info path.

    // Set scale to 1.0 for CPU 0
    // 12 bytes: u32 cpu + u32 integral + u32 fractional
    let scale_one: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]; // cpu=0, {1, 0}
    let result = sc.sys_system_set_performance_info(
        cpu_resource,
        ZX_CPU_PERF_SCALE,
        scale_one.as_ptr() as usize,
        1,
    );
    assert_eq!(result, Ok(()));

    // Set scale to 0.5 for CPU 0: {integral=0, fractional=0x80000000}
    let scale_half: [u8; 12] = [
        0, 0, 0, 0, // cpu=0
        0, 0, 0, 0, // integral=0
        0, 0, 0, 0x80, // fractional=0x80000000
    ];
    let result = sc.sys_system_set_performance_info(
        cpu_resource,
        ZX_CPU_PERF_SCALE,
        scale_half.as_ptr() as usize,
        1,
    );
    // fractional = 0x80000000, integral = 0 → not zero, should succeed.
    assert_eq!(result, Ok(()));

    // Restore scale to 1.0
    let result = sc.sys_system_set_performance_info(
        cpu_resource,
        ZX_CPU_PERF_SCALE,
        scale_one.as_ptr() as usize,
        1,
    );
    assert_eq!(result, Ok(()));
}

/// C++: TEST(SystemCpu, ScaleBandwidth)
///
/// ```cpp
/// // TODO(https://fxbug.dev/42166790): Disabled while flakes are investigated.
/// return;
/// ```
///
/// The C++ test body begins with an unconditional `return;` — it is disabled
/// while flakes are investigated. We faithfully port the disabled test.
/// When upstream re-enables it, this test would need to:
/// 1. Set performance scale to 0.5 on the test CPU
/// 2. Run a deadline-scheduled thread with known capacity/period
/// 3. Measure actual CPU time and verify it scales with the performance scale
#[test]
fn scale_bandwidth() {
    // C++: return; (disabled while flakes are investigated)
    // Faithfully matches upstream: immediate return, no assertions.
}

/// C++: TEST(SystemCpu, SetPerformanceInfoNearZero)
///
/// ```cpp
/// zx_cpu_performance_info_t info{};
/// info.logical_cpu_number = 0;
/// info.performance_scale.integral_part = 0;
/// info.performance_scale.fractional_part = 1;
/// ASSERT_OK(zx_system_set_performance_info(resource->get(), ZX_CPU_PERF_SCALE, &info, 1));
/// ```
///
/// Set a performance scale near zero (0.0 + epsilon) to verify no
/// divide-by-zero crash.
#[test]
fn set_performance_info_near_zero() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let cpu_resource = add_cpu_resource(&ctx);

    // cpu=0, scale={integral_part=0, fractional_part=1} → near-zero but not zero
    // 12 bytes: u32 cpu + u32 integral + u32 fractional
    let info: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
    let result = sc.sys_system_set_performance_info(
        cpu_resource,
        ZX_CPU_PERF_SCALE,
        info.as_ptr() as usize,
        1,
    );
    // C++: ASSERT_OK — should succeed without crashing
    assert_eq!(result, Ok(()));
}
