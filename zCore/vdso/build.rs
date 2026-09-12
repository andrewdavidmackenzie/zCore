//! Build script for the vDSO crate.
//!
//! Generates architecture-specific syscall trampoline assembly.

use std::env;
use std::fs;
use std::path::PathBuf;

/// All Zircon syscalls with their numbers.
/// Matches zx-syscall-numbers.h and SyscallType enum.
const SYSCALLS: &[(&str, u32)] = &[
    ("zx_bti_create", 0),
    ("zx_bti_pin", 1),
    ("zx_bti_release_quarantine", 2),
    ("zx_channel_create", 3),
    ("zx_channel_read", 4),
    ("zx_channel_read_etc", 5),
    ("zx_channel_write", 6),
    ("zx_channel_write_etc", 7),
    ("zx_channel_call_noretry", 8),
    ("zx_channel_call_finish", 9),
    ("zx_clock_get", 10),
    ("zx_clock_adjust", 11),
    ("zx_clock_get_monotonic_via_kernel", 12),
    ("zx_clock_create", 13),
    ("zx_clock_read", 14),
    ("zx_clock_get_details", 15),
    ("zx_clock_update", 16),
    ("zx_cprng_draw_once", 17),
    ("zx_cprng_add_entropy", 18),
    ("zx_debug_read", 19),
    ("zx_debug_write", 20),
    ("zx_debug_send_command", 21),
    ("zx_debuglog_create", 22),
    ("zx_debuglog_write", 23),
    ("zx_debuglog_read", 24),
    ("zx_event_create", 25),
    ("zx_eventpair_create", 26),
    ("zx_exception_get_thread", 27),
    ("zx_exception_get_process", 28),
    ("zx_fifo_create", 29),
    ("zx_fifo_read", 30),
    ("zx_fifo_write", 31),
    ("zx_framebuffer_get_info", 32),
    ("zx_framebuffer_set_range", 33),
    ("zx_futex_wait", 34),
    ("zx_futex_wake", 35),
    ("zx_futex_requeue", 36),
    ("zx_futex_wake_single_owner", 37),
    ("zx_futex_requeue_single_owner", 38),
    ("zx_futex_get_owner", 39),
    ("zx_guest_create", 40),
    ("zx_guest_set_trap", 41),
    ("zx_handle_close", 42),
    ("zx_handle_close_many", 43),
    ("zx_handle_duplicate", 44),
    ("zx_handle_replace", 45),
    ("zx_interrupt_create", 46),
    ("zx_interrupt_bind", 47),
    ("zx_interrupt_wait", 48),
    ("zx_interrupt_destroy", 49),
    ("zx_interrupt_ack", 50),
    ("zx_interrupt_trigger", 51),
    ("zx_interrupt_bind_vcpu", 52),
    ("zx_iommu_create", 53),
    ("zx_ioports_request", 54),
    ("zx_ioports_release", 55),
    ("zx_job_create", 56),
    ("zx_job_set_policy", 57),
    ("zx_job_set_critical", 58),
    ("zx_ktrace_read", 59),
    ("zx_ktrace_control", 60),
    ("zx_ktrace_write", 61),
    ("zx_nanosleep", 62),
    ("zx_ticks_get_via_kernel", 63),
    ("zx_msi_allocate", 64),
    ("zx_msi_create", 65),
    ("zx_mtrace_control", 66),
    ("zx_object_wait_one", 67),
    ("zx_object_wait_many", 68),
    ("zx_object_wait_async", 69),
    ("zx_object_signal", 70),
    ("zx_object_signal_peer", 71),
    ("zx_object_get_property", 72),
    ("zx_object_set_property", 73),
    ("zx_object_get_info", 74),
    ("zx_object_get_child", 75),
    ("zx_object_set_profile", 76),
    ("zx_pager_create", 77),
    ("zx_pager_create_vmo", 78),
    ("zx_pager_detach_vmo", 79),
    ("zx_pager_supply_pages", 80),
    ("zx_pager_op_range", 81),
    ("zx_pc_firmware_tables", 82),
    ("zx_pci_get_nth_device", 83),
    ("zx_pci_enable_bus_master", 84),
    ("zx_pci_reset_device", 85),
    ("zx_pci_config_read", 86),
    ("zx_pci_config_write", 87),
    ("zx_pci_cfg_pio_rw", 88),
    ("zx_pci_get_bar", 89),
    ("zx_pci_map_interrupt", 90),
    ("zx_pci_query_irq_mode", 91),
    ("zx_pci_set_irq_mode", 92),
    ("zx_pci_init", 93),
    ("zx_pci_add_subtract_io_range", 94),
    ("zx_pmt_unpin", 95),
    ("zx_port_create", 96),
    ("zx_port_queue", 97),
    ("zx_port_wait", 98),
    ("zx_port_cancel", 99),
    ("zx_process_exit", 100),
    ("zx_process_create", 101),
    ("zx_process_start", 102),
    ("zx_process_read_memory", 103),
    ("zx_process_write_memory", 104),
    ("zx_profile_create", 105),
    ("zx_resource_create", 106),
    ("zx_smc_call", 107),
    ("zx_socket_create", 108),
    ("zx_socket_write", 109),
    ("zx_socket_read", 110),
    ("zx_socket_shutdown", 111),
    ("zx_stream_create", 112),
    ("zx_stream_writev", 113),
    ("zx_stream_writev_at", 114),
    ("zx_stream_readv", 115),
    ("zx_stream_readv_at", 116),
    ("zx_stream_seek", 117),
    ("zx_system_get_event", 129),
    ("zx_system_mexec", 130),
    ("zx_system_mexec_payload_get", 131),
    ("zx_system_powerctl", 132),
    ("zx_task_suspend", 133),
    ("zx_task_suspend_token", 134),
    ("zx_task_create_exception_channel", 135),
    ("zx_task_kill", 136),
    ("zx_thread_exit", 137),
    ("zx_thread_create", 138),
    ("zx_thread_start", 139),
    ("zx_thread_read_state", 140),
    ("zx_thread_write_state", 141),
    ("zx_timer_create", 142),
    ("zx_timer_set", 143),
    ("zx_timer_cancel", 144),
    ("zx_vcpu_create", 145),
    ("zx_vcpu_resume", 146),
    ("zx_vcpu_interrupt", 147),
    ("zx_vcpu_read_state", 148),
    ("zx_vcpu_write_state", 149),
    ("zx_vmar_allocate", 150),
    ("zx_vmar_destroy", 151),
    ("zx_vmar_map", 152),
    ("zx_vmar_unmap", 153),
    ("zx_vmar_protect", 154),
    ("zx_vmar_op_range", 155),
    ("zx_vmo_create", 156),
    ("zx_vmo_read", 157),
    ("zx_vmo_write", 158),
    ("zx_vmo_get_size", 159),
    ("zx_vmo_set_size", 160),
    ("zx_vmo_op_range", 161),
    ("zx_vmo_create_child", 162),
    ("zx_vmo_set_cache_policy", 163),
    ("zx_vmo_replace_as_executable", 164),
    ("zx_vmo_create_contiguous", 165),
    ("zx_vmo_create_physical", 166),
    // Composite syscalls
    ("zx_futex_wake_handle_close_thread_exit", 200),
    ("zx_vmar_unmap_handle_close_thread_exit", 201),
];

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let asm = match target_arch.as_str() {
        "aarch64" => generate_aarch64(),
        "x86_64" => generate_x86_64(),
        "riscv64" => generate_riscv64(),
        _ => String::from("// No trampolines for this architecture\n"),
    };

    // Write Rust global_asm! wrapper for the static library
    let trampolines_path = out_dir.join("trampolines.rs");
    fs::write(
        &trampolines_path,
        format!(
            "core::arch::global_asm!(\n\
             r#\"\n\
             {asm}\
             \"#\n\
             );\n"
        ),
    )
    .unwrap();

    // Also write a standalone assembly file for external assembly/linking.
    // This can be assembled with `as` and linked with `ld` to produce
    // the vDSO flat binary without Rust runtime dependencies.
    let standalone_asm_path = out_dir.join("vdso_trampolines.S");
    fs::write(&standalone_asm_path, &asm).unwrap();
    println!(
        "cargo:warning=vDSO assembly written to {}",
        standalone_asm_path.display()
    );
}

fn generate_aarch64() -> String {
    let mut asm = String::from(
        "// Auto-generated aarch64 vDSO syscall trampolines\n\
         .text\n\
         .balign 4\n\n",
    );
    for (name, num) in SYSCALLS {
        asm.push_str(&format!(
            ".globl {name}\n\
             .type {name}, %function\n\
             {name}:\n\
             \tmov x16, #{num}\n\
             \tsvc #0\n\
             \tret\n\n"
        ));
    }
    asm
}

fn generate_x86_64() -> String {
    let mut asm = String::from(
        "// Auto-generated x86_64 vDSO syscall trampolines\n\
         .text\n\n",
    );
    for (name, num) in SYSCALLS {
        asm.push_str(&format!(
            ".globl {name}\n\
             .type {name}, @function\n\
             {name}:\n\
             \tmov ${num}, %eax\n\
             \tsyscall\n\
             \tret\n\n"
        ));
    }
    asm
}

fn generate_riscv64() -> String {
    let mut asm = String::from(
        "// Auto-generated riscv64 vDSO syscall trampolines\n\
         .text\n\
         .balign 4\n\n",
    );
    for (name, num) in SYSCALLS {
        asm.push_str(&format!(
            ".globl {name}\n\
             .type {name}, @function\n\
             {name}:\n\
             \tli a7, {num}\n\
             \tecall\n\
             \tret\n\n"
        ));
    }
    asm
}
