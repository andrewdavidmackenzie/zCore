fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/userstart.ld", manifest_dir);
    // Build as a static PIE so the linker emits relocation entries
    // (RELA) for all absolute addresses (GOT, vtables, etc.).
    // The kernel applies these relocations after mapping the ELF,
    // using the VMAR base as the load bias. This is necessary because
    // the ELF is linked at virtual address 0 but loaded at a non-zero
    // VMAR base (e.g. 0x200000 on x86_64, 0x4_0000_0000 on aarch64).
    println!("cargo:rustc-link-arg=--pie");
    println!("cargo:rustc-link-arg=-static");

    println!("cargo:rerun-if-changed=userstart.ld");
}
