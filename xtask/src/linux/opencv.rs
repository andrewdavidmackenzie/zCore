use super::join_path_env;
use crate::{commands::fetch_online, Arch, REPOS};
use os_xtask_utils::{dir, CommandExt, Ext, Git, Make};
use std::{fs, path::Path};

impl super::LinuxRootfs {
    pub fn put_ffmpeg(&self) {
        // Recursively build rootfs
        let musl = self.put_musl_libs();
        // Clone ffmpeg
        let ffmpeg = REPOS.join("ffmpeg");
        if !ffmpeg.is_dir() {
            fetch_online!(ffmpeg, |tmp| {
                Git::clone("https://github.com/FFmpeg/FFmpeg.git")
                    .dir(tmp)
                    .branch("release/5.0")
                    .single_branch()
                    .depth(1)
                    .done()
            });
        }
        // Copy to target path
        let build = self.0.target().join("ffmpeg");
        dircpy::copy_dir(ffmpeg, &build).unwrap();
        // Build
        match self.0 {
            Arch::Riscv64 => {
                let path_with_musl_gcc = join_path_env(&[musl.join("bin")]);
                println!("Configuring ffmpeg, please wait...");
                Ext::new("./configure")
                    .current_dir(&build)
                    .arg("--enable-cross-compile")
                    .arg("--cross-prefix=riscv64-linux-musl-")
                    .arg("--arch=riscv64")
                    .arg("--target-os=linux")
                    .arg("--enable-static")
                    .arg("--enable-shared")
                    .arg("--disable-doc")
                    .arg(format!(
                        "--prefix={}",
                        build.canonicalize().unwrap().join("install").display(),
                    ))
                    .env("PATH", &path_with_musl_gcc)
                    .invoke();
                Make::install()
                    .current_dir(&build)
                    .j(num_cpus::get().min(8)) // Limit threads to avoid running out of memory
                    .env("PATH", path_with_musl_gcc)
                    .invoke();
            }
            Arch::X86_64 | Arch::Aarch64 => todo!(),
        }
        // Copy libraries
        self.put_libs(musl, build.join("install"));
    }

    pub fn put_opencv(&self) {
        // Recursively build rootfs
        let musl = self.put_musl_libs();
        // Clone opencv
        let opencv = REPOS.join("opencv");
        if !opencv.is_dir() {
            fetch_online!(opencv, |tmp| {
                Git::clone("https://github.com/opencv/opencv.git")
                    .dir(tmp)
                    .single_branch()
                    .depth(1)
                    .done()
            });
        }
        let source = opencv.canonicalize().unwrap();
        let target = self.0.target().join("opencv");
        // Re-run cmake if no Makefile was generated
        let cmake_needed = !target.join("Makefile").is_file();
        // Run make if cmake was executed or the install directory does not exist
        let install_needed = cmake_needed || !target.join("install").is_dir();
        // Toolchain
        let path_with_musl_gcc = join_path_env(&[musl.join("bin")]);
        //
        if cmake_needed {
            dir::clear(&target).unwrap();
            // ffmpeg path
            let ffmpeg = self.0.target().join("ffmpeg").join("install").join("lib");
            // Create platform-specific cmake toolchain file
            let platform_cmake = self.0.target().join("musl-gcc.toolchain.cmake");
            fs::write(&platform_cmake, self.opencv_cmake(&ffmpeg)).unwrap();
            // Execute
            let mut cmake = Ext::new("cmake");
            if ffmpeg.is_dir() {
                cmake.env(
                    "PKG_CONFIG_LIBDIR",
                    ffmpeg.join("pkgconfig").canonicalize().unwrap(),
                );
            }
            cmake
                .current_dir(&target)
                .arg(format!(
                    "-DCMAKE_TOOLCHAIN_FILE={}",
                    platform_cmake.canonicalize().unwrap().display()
                ))
                .arg("-DWITH_FFMPEG=ON")
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .arg(format!(
                    "-DCMAKE_INSTALL_PREFIX={}",
                    target.canonicalize().unwrap().join("install").display(),
                ))
                .arg(source)
                .env("PATH", &path_with_musl_gcc)
                .invoke();
        }
        //
        if install_needed {
            Make::install()
                .current_dir(&target)
                .j(num_cpus::get().min(8)) // Limit threads to avoid running out of memory
                .env("PATH", path_with_musl_gcc)
                .invoke();
        }
        // Copy libraries
        self.put_libs(musl, target.join("install"));
    }

    /// Generates a cmake toolchain file for the opencv build.
    fn opencv_cmake(&self, ffmpeg: impl AsRef<Path>) -> String {
        // cmake is tricky
        if !matches!(self.0, Arch::Riscv64) {
            todo!();
        }
        const HEAD: &str = "\
set(CMAKE_SYSTEM_NAME      \"Linux\")
set(CMAKE_SYSTEM_PROCESSOR \"riscv64\")

set(CMAKE_C_COMPILER   riscv64-linux-musl-gcc)
set(CMAKE_CXX_COMPILER riscv64-linux-musl-g++)

set(CMAKE_C_FLAGS   \"\" CACHE STRING \"\")
set(CMAKE_CXX_FLAGS \"\" CACHE STRING \"\")

set(CMAKE_C_FLAGS   \"-march=rv64gc ${CMAKE_C_FLAGS}   ${CMAKE_PASS_TEST_FLAGS}\")
set(CMAKE_CXX_FLAGS \"-march=rv64gc ${CMAKE_CXX_FLAGS} ${CMAKE_PASS_TEST_FLAGS}\")";

        let ffmpeg = ffmpeg.as_ref();
        if ffmpeg.is_dir() {
            format!(
                "\
{HEAD}

set(CMAKE_LD_FFMPEG_FLAGS  \"-Wl,-rpath-link,{}\")
set(CMAKE_EXE_LINKER_FLAGS \"${{CMAKE_EXE_LINKER_FLAGS}} ${{CMAKE_LD_FFMPEG_FLAGS}}\")",
                ffmpeg.canonicalize().unwrap().display()
            )
        } else {
            HEAD.into()
        }
    }
}
