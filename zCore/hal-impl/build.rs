fn main() {
    let max_cpus: usize = std::env::var("ZCORE_MAX_CPUS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4); // default to 4 if not set

    println!("cargo:rustc-cfg=max_cpus=\"{max_cpus}\"");
    println!("cargo:rustc-env=MAX_CPUS={max_cpus}");
    println!("cargo:rerun-if-env-changed=ZCORE_MAX_CPUS");
    // ZCORE_CMDLINE is embedded at compile time via option_env!() in
    // arch-specific entry points. Without this, changing the command
    // line (e.g., different CORE_TESTS_FILTER) may reuse a cached
    // kernel with the old value baked in.
    println!("cargo:rerun-if-env-changed=ZCORE_CMDLINE");

    // Track the linker script so that changes to section layout or
    // load addresses trigger a kernel rebuild.
    println!("cargo:rerun-if-env-changed=ZCORE_LINKER_SCRIPT");
    if let Ok(script) = std::env::var("ZCORE_LINKER_SCRIPT") {
        if !script.is_empty() {
            println!("cargo:rerun-if-changed={script}");
        }
    }
}
