mod image;
mod opencv;
mod test;

use crate::{commands::fetch_online, Arch, PROJECT_DIR, REPOS};
use os_xtask_utils::{dir, CommandExt, Ext, Git, Make};
use std::{
    env,
    ffi::OsString,
    fs,
    os::unix,
    path::{Path, PathBuf},
};

pub(crate) struct LinuxRootfs(Arch);

impl LinuxRootfs {
    /// Creates a linux rootfs handler for the specified architecture.
    #[inline]
    pub const fn new(arch: Arch) -> Self {
        Self(arch)
    }

    /// Builds the boot rootfs.
    /// For x86_64, this filesystem can be used for libos boot.
    /// If `clear` is set, the existing directory will be removed.
    pub fn make(&self, clear: bool) {
        let dir = self.path();
        if dir.is_dir() && !clear {
            // Verify the cached rootfs has busybox with the correct link mode.
            // On aarch64 macOS we need static-pie; everywhere else plain static.
            let bb = dir.join("bin").join("busybox");
            if bb.is_file() {
                let output = std::process::Command::new("file")
                    .arg(&bb)
                    .output()
                    .expect("failed to run `file`");
                let desc = String::from_utf8_lossy(&output.stdout);
                // Regular rootfs always uses non-PIE static busybox.
                if desc.contains("statically linked") {
                    return;
                }
                println!("cached rootfs busybox is not statically linked, rebuilding...");
            } else {
                println!("cached rootfs is missing busybox, rebuilding...");
            }
        }
        // Prepare resources needed for the minimal system
        let musl = self.0.linux_musl_cross();
        let busybox = self.busybox(&musl);
        // Create target directories
        let bin = dir.join("bin");
        let lib = dir.join("lib");
        dir::clear(&dir).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::create_dir(&lib).unwrap();
        // Copy busybox
        fs::copy(busybox, bin.join("busybox")).unwrap();
        // Copy libc.so
        let from = musl
            .join(format!("{}-linux-musl", self.0.name()))
            .join("lib")
            .join("libc.so");
        let to = lib.join(format!("ld-musl-{arch}.so.1", arch = self.0.name()));
        fs::copy(from, &to).unwrap();
        Ext::new(self.strip(musl)).arg("-s").arg(to).invoke();
        // Create symlinks for common utilities
        const SH: &[&str] = &[
            "cat", "cp", "echo", "false", "grep", "gzip", "halt", "kill", "ln", "ls", "mkdir",
            "mv", "pidof", "ping", "ping6", "poweroff", "printenv", "ps", "pwd", "reboot", "rm",
            "rmdir", "sh", "sleep", "stat", "tar", "timeout", "touch", "true", "uname", "usleep",
            "watch",
        ];
        let bin = dir.join("bin");
        for sh in SH {
            unix::fs::symlink("busybox", bin.join(sh)).unwrap();
        }
    }

    /// Copies musl shared libraries into rootfs.
    pub fn put_musl_libs(&self) -> PathBuf {
        // Recursively build rootfs
        self.make(false);
        let dir = self.0.linux_musl_cross();
        self.put_libs(&dir, dir.join(format!("{}-linux-musl", self.0.name())));
        dir
    }

    /// Returns the rootfs path for the specified architecture.
    /// Layout: `rootfs/linux/{arch}/`
    #[inline]
    pub fn path(&self) -> PathBuf {
        PROJECT_DIR.join("rootfs").join("linux").join(self.0.name())
    }

    /// Returns the libos-specific rootfs path.
    /// Layout: `rootfs/linux-libos/{arch}/`
    /// Used on aarch64 macOS where a static-PIE busybox is needed.
    #[inline]
    pub fn libos_path(&self) -> PathBuf {
        PROJECT_DIR
            .join("rootfs")
            .join("linux-libos")
            .join(self.0.name())
    }

    /// Returns true if the libos on this host requires a static-PIE busybox.
    ///
    /// On aarch64 macOS, `mmap(MAP_FIXED)` fails for addresses below
    /// ~0x400000000. The libos loader relocates ELF binaries above this
    /// threshold, but non-PIE static binaries use absolute addresses that
    /// can't be fixed up without relocation info. A static-PIE binary
    /// includes `.rela.dyn` entries and self-relocates via rcrt1 at
    /// startup, so it works at any load address.
    ///
    /// Note: the regular `busybox()` method always builds non-PIE static.
    /// This flag is only used by `make_libos()` to build a separate rootfs.
    fn needs_libos_static_pie(&self) -> bool {
        cfg!(all(target_os = "macos", target_arch = "aarch64")) && matches!(self.0, Arch::Aarch64)
    }

    /// Build a separate libos rootfs with static-PIE busybox.
    /// Only builds on platforms that need it (aarch64 macOS).
    /// On other platforms, the regular rootfs is used for libos too.
    pub fn make_libos(&self, clear: bool) {
        // Always build the regular rootfs first (needed for bare-metal
        // and as a fallback for libos on platforms that don't need PIE).
        self.make(clear);

        if !self.needs_libos_static_pie() {
            return; // Regular rootfs is fine for libos on this platform.
        }

        let dir = self.libos_path();
        if dir.is_dir() && !clear {
            let bb = dir.join("bin").join("busybox");
            if bb.is_file() {
                let output = std::process::Command::new("file")
                    .arg(&bb)
                    .output()
                    .expect("failed to run `file`");
                let desc = String::from_utf8_lossy(&output.stdout);
                if desc.contains("static-pie linked") {
                    return; // Already built.
                }
                println!("cached libos rootfs busybox is not static-pie, rebuilding...");
            }
        }

        println!("Building libos rootfs with static-PIE busybox...");

        // Copy the regular rootfs as a base
        let src = self.path();
        dir::clear(&dir).unwrap();
        dircpy::copy_dir(&src, &dir).unwrap();

        // Build static-PIE busybox and replace the regular one
        let musl = self.0.linux_musl_cross();
        let busybox = self.busybox_static_pie(&musl);
        fs::copy(busybox, dir.join("bin").join("busybox")).unwrap();
    }

    /// Cross-compiles busybox as static-PIE.
    fn busybox_static_pie(&self, musl: impl AsRef<Path>) -> PathBuf {
        let target = self.0.target().join("busybox-pie");
        let executable = target.join("busybox");
        if executable.is_file() {
            let output = std::process::Command::new("file")
                .arg(&executable)
                .output()
                .expect("failed to run `file`");
            let desc = String::from_utf8_lossy(&output.stdout);
            if desc.contains("static-pie linked") {
                return executable;
            }
            println!("cached static-pie busybox has wrong type, rebuilding...");
            dir::rm(&target).unwrap();
        }
        // Copy busybox source (already cloned by the regular busybox() build)
        let source = REPOS.join("busybox");
        if !source.is_dir() {
            // The regular busybox() already cloned it; just verify.
            panic!(
                "busybox source not found at {:?} -- run regular rootfs build first",
                source
            );
        }
        dir::rm(&target).unwrap();
        dircpy::copy_dir(&source, &target).unwrap();
        // Configure
        Make::new().current_dir(&target).arg("defconfig").invoke();
        let config_path = target.join(".config");
        let config = fs::read_to_string(&config_path).expect("failed to read .config");
        let mut config = config.replace("# CONFIG_STATIC is not set", "CONFIG_STATIC=y");
        config = config.replace(
            r#"CONFIG_EXTRA_CFLAGS="""#,
            r#"CONFIG_EXTRA_CFLAGS="-fpie""#,
        );
        fs::write(&config_path, config).expect("failed to write .config");
        // Compile with static-PIE flags
        let musl = musl.as_ref();
        let mut make = Make::new();
        make.current_dir(&target).arg(format!(
            "CROSS_COMPILE={musl}/{arch}-linux-musl-",
            musl = musl.canonicalize().unwrap().join("bin").display(),
            arch = self.0.name(),
        ));
        make.arg("CFLAGS_busybox=-static-pie -Wl,-z,notext");
        make.invoke();
        // Strip
        Ext::new(self.strip(musl))
            .arg("-s")
            .arg(&executable)
            .invoke();
        executable
    }

    /// Cross-compiles busybox as a non-PIE static binary.
    /// Used for the regular rootfs (bare-metal and non-macOS libos).
    fn busybox(&self, musl: impl AsRef<Path>) -> PathBuf {
        // Final file path
        let target = self.0.target().join("busybox");
        let executable = target.join("busybox");
        // If a cached binary exists, verify it is statically linked.
        if executable.is_file() {
            let output = std::process::Command::new("file")
                .arg(&executable)
                .output()
                .expect("failed to run `file`");
            let desc = String::from_utf8_lossy(&output.stdout);
            if desc.contains("statically linked") {
                return executable;
            }
            println!("cached busybox is not statically linked, rebuilding...");
            dir::rm(&target).unwrap();
        }
        // Fetch source code (use GitHub mirror — the official git.busybox.net
        // server is frequently unreachable from CI runners).
        // Pin to release tag 1_36_1 to avoid build failures from
        // unstable master (e.g. sha1_process_block64_shaNI).
        const BUSYBOX_TAG: &str = "1_36_1";
        let source = REPOS.join("busybox");
        // If an existing checkout is not at the expected tag, remove it
        // so it gets re-cloned at the pinned version.
        if source.is_dir() {
            let tag_file = source.join(".busybox_tag");
            let current = std::fs::read_to_string(&tag_file).unwrap_or_default();
            if current.trim() != BUSYBOX_TAG {
                println!("busybox checkout not at {BUSYBOX_TAG}, re-cloning...");
                dir::rm(&source).unwrap();
            }
        }
        if !source.is_dir() {
            fetch_online!(source, |tmp| {
                Git::clone("https://github.com/mirror/busybox.git")
                    .dir(tmp)
                    .single_branch()
                    .branch(BUSYBOX_TAG)
                    .depth(1)
                    .done()
            });
            // Record the tag so future runs can validate the checkout.
            std::fs::write(source.join(".busybox_tag"), BUSYBOX_TAG)
                .expect("failed to write .busybox_tag");
        }
        // Copy
        dir::rm(&target).unwrap();
        dircpy::copy_dir(source, &target).unwrap();
        // Configure
        Make::new().current_dir(&target).arg("defconfig").invoke();
        // Enable static linking to avoid dynamic linker dependencies.
        // This is essential for bare-metal OS kernels that may not fully
        // implement the mmap semantics required by musl's dynamic linker.
        let config_path = target.join(".config");
        let config = fs::read_to_string(&config_path).expect("failed to read .config");
        let config = config.replace("# CONFIG_STATIC is not set", "CONFIG_STATIC=y");
        fs::write(&config_path, config).expect("failed to write .config");
        // Compile
        let musl = musl.as_ref();
        Make::new()
            .current_dir(&target)
            .arg(format!(
                "CROSS_COMPILE={musl}/{arch}-linux-musl-",
                musl = musl.canonicalize().unwrap().join("bin").display(),
                arch = self.0.name(),
            ))
            .invoke();
        // Strip
        Ext::new(self.strip(musl))
            .arg("-s")
            .arg(&executable)
            .invoke();
        executable
    }

    fn strip(&self, musl: impl AsRef<Path>) -> PathBuf {
        musl.as_ref()
            .join("bin")
            .join(format!("{}-linux-musl-strip", self.0.name()))
    }

    /// Copies all shared libraries and their symlinks from the install directory to rootfs.
    fn put_libs(&self, musl: impl AsRef<Path>, dir: impl AsRef<Path>) {
        let lib = self.path().join("lib");
        let musl_libc_protected = format!("ld-musl-{}.so.1", self.0.name());
        let musl_libc_ignored = "libc.so";
        let strip = self.strip(musl);
        dir.as_ref()
            .join("lib")
            .read_dir()
            .unwrap()
            .filter_map(|res| res.map(|e| e.path()).ok())
            .filter(|path| check_so(path))
            .for_each(|source| {
                let name = source.file_name().unwrap();
                let target = lib.join(name);
                if source.is_symlink() {
                    if name != musl_libc_protected.as_str() {
                        dir::rm(&target).unwrap();
                        // `fs::copy` copies file contents (not symlinks)
                        unix::fs::symlink(source.read_link().unwrap(), target).unwrap();
                    }
                } else if name != musl_libc_ignored {
                    dir::rm(&target).unwrap();
                    fs::copy(source, &target).unwrap();
                    Ext::new(&strip).arg("-s").arg(target).status();
                }
            });
    }
}

/// Appends paths to the PATH environment variable.
fn join_path_env<I, S>(paths: I) -> OsString
where
    I: IntoIterator<Item = S>,
    S: AsRef<Path>,
{
    let mut path = OsString::new();
    let mut first = true;
    if let Ok(current) = env::var("PATH") {
        path.push(current);
        first = false;
    }
    for item in paths {
        if first {
            first = false;
        } else {
            path.push(":");
        }
        path.push(item.as_ref().canonicalize().unwrap().as_os_str());
    }
    path
}

/// Checks if a file is a shared library or a symlink to one.
fn check_so<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    // Must be a symlink or a regular file.
    // For symlinks, `is_file`, `exists`, etc. check the target file.
    if !path.is_symlink() && !path.is_file() {
        return false;
    }
    // Split the filename into segments
    let name = path.file_name().unwrap().to_string_lossy();
    let mut seg = name.split('.');
    // Must not start with '.'
    if matches!(seg.next(), Some("") | None) {
        return false;
    }
    // First extension segment must be "so"
    if !matches!(seg.next(), Some("so")) {
        return false;
    }
    // Everything after "so" must be decimal digits (version numbers)
    !seg.any(|it| !it.chars().all(|ch| ch.is_ascii_digit()))
}
