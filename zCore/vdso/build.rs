//! Build script for the vDSO crate.
//!
//! Generates architecture-specific syscall trampoline assembly from
//! the Fuchsia zx-syscall-numbers.h header. Each exported function
//! has both `zx_foo` and `_zx_foo` names (Fuchsia convention).

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let header_path = "../zircon-syscall/src/zx-syscall-numbers.h";
    println!("cargo:rerun-if-changed={}", header_path);

    // Parse syscall names and numbers from the header.
    let header = fs::read_to_string(header_path).expect("failed to read zx-syscall-numbers.h");
    let mut syscalls: Vec<(String, u32)> = Vec::new();
    for line in header.lines() {
        if !line.starts_with("#define ZX_SYS_") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[1].strip_prefix("ZX_SYS_").unwrap();
        let num: u32 = match parts[2].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        // Skip COUNT and test syscalls
        if name == "COUNT" || name.starts_with("syscall_test") {
            continue;
        }
        syscalls.push((name.to_string(), num));
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let asm = match target_arch.as_str() {
        "aarch64" => generate_aarch64(&syscalls),
        "x86_64" => generate_x86_64(&syscalls),
        "riscv64" => generate_riscv64(&syscalls),
        _ => String::from("// No trampolines for this architecture\n"),
    };

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

    let standalone_asm_path = out_dir.join("vdso_trampolines.S");
    fs::write(&standalone_asm_path, &asm).unwrap();
    println!(
        "cargo:warning=vDSO assembly written to {}",
        standalone_asm_path.display()
    );
}

fn generate_aarch64(syscalls: &[(String, u32)]) -> String {
    let mut asm = String::from(
        "// Auto-generated aarch64 vDSO syscall trampolines (Fuchsia numbering)\n\
         .text\n\
         .balign 4\n\n",
    );
    for (name, num) in syscalls {
        // Export both zx_foo and _zx_foo (Fuchsia convention).
        let zx_name = format!("zx_{}", name);
        let _zx_name = format!("_zx_{}", name);
        asm.push_str(&format!(
            ".globl {zx_name}\n\
             .globl {_zx_name}\n\
             .type {zx_name}, %function\n\
             .type {_zx_name}, %function\n\
             {zx_name}:\n\
             {_zx_name}:\n\
             \tmov x16, #{num}\n\
             \tsvc #0\n\
             \tret\n\n"
        ));
    }
    asm
}

fn generate_x86_64(syscalls: &[(String, u32)]) -> String {
    let mut asm = String::from(
        "// Auto-generated x86_64 vDSO syscall trampolines (Fuchsia numbering)\n\
         .text\n\n",
    );
    for (name, num) in syscalls {
        let zx_name = format!("zx_{}", name);
        let _zx_name = format!("_zx_{}", name);
        asm.push_str(&format!(
            ".globl {zx_name}\n\
             .globl {_zx_name}\n\
             .type {zx_name}, @function\n\
             .type {_zx_name}, @function\n\
             {zx_name}:\n\
             {_zx_name}:\n\
             \tmov ${num}, %eax\n\
             \tsyscall\n\
             \tret\n\n"
        ));
    }
    asm
}

fn generate_riscv64(syscalls: &[(String, u32)]) -> String {
    let mut asm = String::from(
        "// Auto-generated riscv64 vDSO syscall trampolines (Fuchsia numbering)\n\
         .text\n\
         .balign 4\n\n",
    );
    for (name, num) in syscalls {
        let zx_name = format!("zx_{}", name);
        let _zx_name = format!("_zx_{}", name);
        asm.push_str(&format!(
            ".globl {zx_name}\n\
             .globl {_zx_name}\n\
             .type {zx_name}, @function\n\
             .type {_zx_name}, @function\n\
             {zx_name}:\n\
             {_zx_name}:\n\
             \tli a7, {num}\n\
             \tecall\n\
             \tret\n\n"
        ));
    }
    asm
}
