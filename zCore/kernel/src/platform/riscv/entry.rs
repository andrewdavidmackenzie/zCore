use super::{
    boot_page_table::BootPageTable,
    consts::{kernel_mem_info, MAX_HART_NUM, STACK_PAGES_PER_HART},
};
use core::arch::naked_asm;
use dtb_walker::{Dtb, DtbObj, HeaderError::*, Property, Str, WalkOperation::*};
use kernel_hal::KernelConfig;

/// Kernel entry point.
///
/// # Safety
///
/// Naked function.
#[unsafe(naked)]
#[no_mangle]
#[link_section = ".text.entry"]
unsafe extern "C" fn _start(hartid: usize, device_tree_paddr: usize) -> ! {
    naked_asm!(
        "call {select_stack}", // set up boot stack
        "j    {main}",         // enter Rust
        select_stack = sym select_stack,
        main         = sym primary_rust_main)
}

/// Secondary hart entry point. Previously the secondary hart was blocked by the bootloader/SEE.
///
/// # Safety
///
/// Naked function.
#[unsafe(naked)]
unsafe extern "C" fn secondary_hart_start(hartid: usize) -> ! {
    naked_asm!(
        "call {select_stack}", // set up boot stack
        "j    {main}",         // enter Rust
        select_stack = sym select_stack,
        main         = sym secondary_rust_main)
}

/// Boot page table
static mut BOOT_PAGE_TABLE: BootPageTable = BootPageTable::ZERO;

/// Primary hart boot.
extern "C" fn primary_rust_main(hartid: usize, device_tree_paddr: usize) -> ! {
    // Zero the BSS segment
    extern "C" {
        static mut sbss: u64;
        static mut ebss: u64;
    }
    unsafe { r0::zero_bss(&mut sbss, &mut ebss) };
    // Enable the boot page table
    let sstatus = unsafe {
        BOOT_PAGE_TABLE.init();
        BOOT_PAGE_TABLE.launch()
    };
    let mem_info = kernel_mem_info();
    // Verify the device tree
    let dtb = unsafe {
        Dtb::from_raw_parts_filtered((device_tree_paddr + mem_info.offset()) as _, |e| {
            matches!(e, Misaligned(4) | LastCompVersion(_))
        })
    }
    .unwrap();
    // Print boot information
    println!(
        "
boot page table launched, sstatus = {sstatus:#x}
kernel (physical): {:016x}..{:016x}
kernel (remapped): {:016x}..{:016x}
device tree:       {device_tree_paddr:016x}..{:016x}
",
        mem_info.paddr_base,
        mem_info.paddr_base + mem_info.size,
        mem_info.vaddr_base,
        mem_info.vaddr_base + mem_info.size,
        device_tree_paddr + dtb.total_size(),
    );
    // Boot secondary harts
    boot_secondary_harts(
        hartid,
        &dtb,
        secondary_hart_start as *const () as usize - mem_info.offset(),
    );
    // Transfer control
    crate::primary_main(KernelConfig {
        phys_to_virt_offset: mem_info.offset(),
        dtb_paddr: device_tree_paddr,
        dtb_size: dtb.total_size() as _,
    });
    sbi_rt::system_reset(sbi_rt::Shutdown, sbi_rt::NoReason);
    unreachable!()
}

/// Secondary hart boot.
extern "C" fn secondary_rust_main() -> ! {
    let _ = unsafe { BOOT_PAGE_TABLE.launch() };
    crate::secondary_main()
}

/// Set up the boot stack based on the hardware thread ID.
///
/// # Safety
///
/// Naked function.
#[unsafe(naked)]
unsafe extern "C" fn select_stack(hartid: usize) {
    const STACK_LEN_PER_HART: usize = 4096 * STACK_PAGES_PER_HART;
    const STACK_LEN_TOTAL: usize = STACK_LEN_PER_HART * MAX_HART_NUM;
    #[link_section = ".bss.bootstack"]
    static mut BOOT_STACK: [u8; STACK_LEN_TOTAL] = [0u8; STACK_LEN_TOTAL];

    naked_asm!(
        "   mv   tp, a0",
        "   addi t0, a0,  1
            la   sp, {stack}
            li   t1, {len_per_hart}
         1: add  sp, sp, t1
            addi t0, t0, -1
            bnez t0, 1b
            ret
        ",
        stack        =   sym BOOT_STACK,
        len_per_hart = const STACK_LEN_PER_HART)
}

// Boot secondary harts
fn boot_secondary_harts(boot_hartid: usize, dtb: &Dtb, start_addr: usize) {
    if sbi_rt::probe_extension(sbi_rt::Hsm).is_unavailable() {
        println!("HSM SBI extension is not supported for current SEE.");
        return;
    }

    let mut cpus = false;
    let mut cpu: Option<usize> = None;
    dtb.walk(|path, obj| match obj {
        DtbObj::SubNode { name } => {
            if path.is_root() {
                if name == Str::from("cpus") {
                    // Enter the cpus node
                    cpus = true;
                    StepInto
                } else if cpus {
                    // Already left the cpus node
                    if let Some(hartid) = cpu.take() {
                        hart_start(boot_hartid, hartid, start_addr);
                    }
                    Terminate
                } else {
                    // Other nodes
                    StepOver
                }
            } else if path.name() == Str::from("cpus") {
                // If there is no cpu index, it must be single-core
                if name == Str::from("cpu") {
                    return Terminate;
                }
                if name.starts_with("cpu@") {
                    let id: usize = usize::from_str_radix(
                        unsafe { core::str::from_utf8_unchecked(&name.as_bytes()[4..]) },
                        16,
                    )
                    .unwrap();
                    if let Some(hartid) = cpu.replace(id) {
                        hart_start(boot_hartid, hartid, start_addr);
                    }
                    StepInto
                } else {
                    StepOver
                }
            } else {
                StepOver
            }
        }
        // CPUs whose status is not "okay" cannot be started
        DtbObj::Property(Property::Status(status))
            if path.name().starts_with("cpu@") && status != Str::from("okay") =>
        {
            if let Some(id) = cpu.take() {
                println!("hart{id} has status: {status}");
            }
            StepOut
        }
        DtbObj::Property(_) => StepOver,
    });
    println!();
}

fn hart_start(boot_hartid: usize, hartid: usize, start_addr: usize) {
    if hartid != boot_hartid {
        println!("hart{hartid} is booting...");
        let ret = sbi_rt::hart_start(hartid, start_addr, 0);
        if ret.is_err() {
            panic!("start hart{hartid} failed. error: {ret:?}");
        }
    } else {
        println!("hart{hartid} is the primary hart.");
    }
}
