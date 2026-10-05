fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/petal.ld", manifest_dir);
    // Non-PIE static executable. The link base (0x200000) matches
    // USER_ASPACE_BASE so absolute addresses are correct when
    // userstart maps the binary at the VMAR base.
    println!("cargo:rustc-link-arg=--no-pie");
    println!("cargo:rustc-link-arg=-static");
    println!("cargo:rerun-if-changed=petal.ld");
}
