fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // If USERSTART_ELF is not set, generate an empty stub so
    // include_bytes! compiles. The rootfs-based boot path doesn't
    // need an embedded userstart.
    if std::env::var("USERSTART_ELF").is_err() {
        let stub = out.join("empty_userstart.elf");
        std::fs::write(stub.as_path(), b"").unwrap();
        println!("cargo:rustc-env=USERSTART_ELF={}", stub.display());
    }

    // If VDSO_BIN is not set, generate an empty stub.
    // The vDSO code pages will be empty (trampolines not embedded).
    if std::env::var("VDSO_BIN").is_err() {
        let stub = out.join("empty_vdso.bin");
        std::fs::write(stub.as_path(), b"").unwrap();
        println!("cargo:rustc-env=VDSO_BIN={}", stub.display());
    }
}
