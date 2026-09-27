fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/userstart.ld", manifest_dir);
    // Build as non-PIE static executable to avoid GOT/relocation entries.
    // The kernel maps userstart at a fixed VMAR offset and does not apply
    // ELF relocations, so the binary must not require them.
    println!("cargo:rustc-link-arg=--no-pie");
    println!("cargo:rustc-link-arg=-static");
    println!("cargo:rerun-if-changed=userstart.ld");
}
