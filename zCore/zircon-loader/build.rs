fn main() {
    // If USERSTART_ELF is not set, generate an empty stub so
    // include_bytes! compiles. The rootfs-based boot path doesn't
    // need an embedded userstart.
    if std::env::var("USERSTART_ELF").is_err() {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let stub = out.join("empty_userstart.elf");
        std::fs::write(stub.as_path(), b"").unwrap();
        println!("cargo:rustc-env=USERSTART_ELF={}", stub.display());
    }
}
