//! Rootfs image manifests.
//!
//! A product's `rootfs-contents` field names a manifest that decides
//! what goes into the boot rootfs image:
//!
//! - `full` — busybox + musl libc + petal programs + demo binaries
//!   (linux-hello, wasi-runner). The shared demo/test rootfs.
//! - `petal` — petal programs only (Zircon).
//! - `core-tests` — petal programs + prebuilt Fuchsia test binaries
//!   and shared libraries (Zircon).
//!
//! Images are keyed by (manifest, arch) under `target/rootfs/images/`
//! and shared across boards and selections: the image content does not
//! depend on which board boots it.

use crate::{arch::Arch, linux::LinuxRootfs, petal, PROJECT_DIR};
use std::path::PathBuf;

/// A rootfs image manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manifest {
    /// busybox + musl + petal + demo binaries (shared rootfs).
    Full,
    /// Petal programs only (Zircon).
    Petal,
    /// Petal + prebuilt Fuchsia test binaries (Zircon).
    CoreTests,
}

impl Manifest {
    /// Parses a manifest name, panicking with the valid options on error.
    pub fn parse(name: &str) -> Self {
        match name {
            "full" => Self::Full,
            "petal" => Self::Petal,
            "core-tests" => Self::CoreTests,
            other => panic!(
                "unknown rootfs-contents '{other}' — expected 'full', 'petal', or 'core-tests'"
            ),
        }
    }

    /// The manifest name (as used in product TOMLs).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Petal => "petal",
            Self::CoreTests => "core-tests",
        }
    }

    /// The image path for this manifest and architecture.
    pub fn image_path(&self, arch: Arch) -> PathBuf {
        PROJECT_DIR
            .join("target")
            .join("rootfs")
            .join("images")
            .join(format!("{}-{}.img", self.name(), arch.name()))
    }

    /// Builds (or reuses, if up to date) the rootfs image for this
    /// manifest and returns its path.
    pub fn build_image(&self, arch: Arch) -> PathBuf {
        match self {
            Self::Full => {
                let rootfs = LinuxRootfs::new(arch);
                rootfs.image();
                rootfs.image_path()
            }
            Self::Petal | Self::CoreTests => petal::build_zircon_rootfs_image(
                arch,
                *self == Self::CoreTests,
                self.image_path(arch),
            ),
        }
    }
}
