fn main() {
    let max_cpus: usize = std::env::var("ZCORE_MAX_CPUS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4); // default to 4 if not set

    println!("cargo:rustc-env=MAX_CPUS={max_cpus}");
    println!("cargo:rerun-if-env-changed=ZCORE_MAX_CPUS");
}
