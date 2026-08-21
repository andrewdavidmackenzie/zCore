use super::join_path_env;
use crate::{commands::wget, Arch};
use os_xtask_utils::{dir, CommandExt, Ext, Make, Tar};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
};

impl super::LinuxRootfs {
    /// Copies libc-test into rootfs.
    pub fn put_libc_test(&self) {
        // Recursively build rootfs
        self.make(false);
        // Copy repository
        let dir = self.path().join("libc-test");
        dir::rm(&dir).unwrap();
        dircpy::copy_dir("libc-test", &dir).unwrap();
        // Compile
        fs::copy(dir.join("config.mak.def"), dir.join("config.mak")).unwrap();
        Make::new()
            .j(usize::MAX)
            .env("ARCH", self.0.name())
            .env("CROSS_COMPILE", format!("{}-linux-musl-", self.0.name()))
            .env(
                "PATH",
                join_path_env(&[self.0.linux_musl_cross().join("bin")]),
            )
            .current_dir(&dir)
            .invoke();
        // FIXME Why does this need to be replaced?
        if let Arch::Riscv64 = self.0 {
            fs::copy(
                riscv64_special().join("libc-test/functional/tls_align-static.exe"),
                dir.join("src/functional/tls_align-static.exe"),
            )
            .unwrap();
        }

        // Remove unnecessary libc-test files
        let elf_path = OsString::from("src");
        let test_set = HashSet::from([
            OsString::from("api"),
            OsString::from("common"),
            OsString::from("math"),
            OsString::from("musl"),
            OsString::from("functional"),
            OsString::from("regression"),
        ]);

        fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|path| path.file_name() != elf_path)
            .for_each(|path| dir::rm(path.path()).unwrap());

        fs::read_dir(dir.join(&elf_path))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|path| !test_set.contains(&path.file_name()))
            .for_each(|path| dir::rm(path.path()).unwrap());

        for item in test_set {
            fs::read_dir(dir.join(&elf_path).join(item))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|path| !path.file_name().into_string().unwrap().ends_with(".exe"))
                .filter(|path| !path.file_name().into_string().unwrap().ends_with(".so"))
                .for_each(|path| dir::rm(path.path()).unwrap());
        }
    }

    /// Copies other tests into rootfs.
    pub fn put_other_test(&self) {
        // Recursively build rootfs
        self.make(false);
        // Build linux-syscall/test
        let bin = self.path().join("bin");
        let musl_cross = self
            .0
            .linux_musl_cross()
            .join("bin")
            .join(format!("{}-linux-musl-gcc", self.0.name()));
        fs::read_dir("linux-syscall/test")
            .unwrap()
            .filter_map(|res| res.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == OsStr::new("c")))
            .for_each(|c| {
                Ext::new(&musl_cross)
                    .arg(&c)
                    .arg("-o")
                    .arg(bin.join(c.file_stem().unwrap()))
                    .invoke()
            });
        // Also add oscomp for riscv64
        if let Arch::Riscv64 = self.0 {
            dircpy::copy_dir(riscv64_special().join("oscomp"), self.path().join("oscomp")).unwrap();
        }
    }
}

fn riscv64_special() -> PathBuf {
    const URL: &str =
        "https://github.com/rcore-os/libc-test-prebuilt/releases/download/0.1/prebuild.tar.xz";
    let tar = Arch::Riscv64.origin().join("prebuild.tar.xz");
    wget(URL, &tar);
    // Extract to target path
    let dir = Arch::Riscv64.target();
    dir::clear(&dir).unwrap();
    Tar::xf(&tar, Some(&dir)).invoke();
    dir.join("prebuild")
}
