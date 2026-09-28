//! Build script for zircon-abi.
//!
//! Generates `SYS_*` syscall number constants from the shared
//! `zx-syscall-numbers.h` header so they stay in sync with the kernel
//! dispatcher (`zircon-syscall/src/consts.rs`).

use std::io::Write;

fn main() {
    let header = std::path::Path::new("../zircon-syscall/src/zx-syscall-numbers.h");
    println!("cargo:rerun-if-changed={}", header.display());

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let out_path = out_dir.join("syscall_numbers.rs");
    let mut out = std::fs::File::create(&out_path).unwrap();

    let data = std::fs::read_to_string(header)
        .expect("could not read zx-syscall-numbers.h — is the workspace intact?");

    for line in data.lines() {
        if !line.starts_with("#define ZX_SYS_") {
            continue;
        }
        let mut iter = line.split(' ');
        let _ = iter.next().unwrap(); // #define
        let name = iter.next().unwrap(); // ZX_SYS_xxx
        let id = match iter.next() {
            Some(id) => id,
            None => continue,
        };
        // Skip COUNT
        if name == "ZX_SYS_COUNT" {
            continue;
        }
        let const_name = name[7..].to_uppercase(); // strip "ZX_SYS_"
        writeln!(out, "pub const SYS_{const_name}: u32 = {id};").unwrap();
    }
}
