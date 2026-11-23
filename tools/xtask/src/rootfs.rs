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
//! - `fuchsia` — no SFS rootfs; Fuchsia userspace artifacts imported
//!   into `prebuilt/fuchsia/<arch>/` by `make fuchsia-import` (Zircon).
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
    /// Fuchsia userspace artifacts (issue #523): no SFS rootfs is
    /// built. The kernel boots Fuchsia's real userboot from artifacts
    /// imported into `prebuilt/fuchsia/<arch>/` by `make fuchsia-import`
    /// (userboot.elf, vdso.elf, fuchsia.zbi). `build_image` validates
    /// the artifacts and returns the ZBI path instead of building.
    Fuchsia,
}

/// Directory holding the imported Fuchsia userspace artifacts for an
/// arch: `prebuilt/fuchsia/<arch>/`. Populated by
/// `tools/scripts/fuchsia-import.sh` (`make fuchsia-import`).
pub fn fuchsia_prebuilt_dir(arch: Arch) -> PathBuf {
    PROJECT_DIR
        .join("prebuilt")
        .join("fuchsia")
        .join(arch.name())
}

impl Manifest {
    /// Parses a manifest name, panicking with the valid options on error.
    pub fn parse(name: &str) -> Self {
        match name {
            "full" => Self::Full,
            "petal" => Self::Petal,
            "core-tests" => Self::CoreTests,
            "fuchsia" => Self::Fuchsia,
            other => panic!(
                "unknown rootfs-contents '{other}' — expected 'full', 'petal', 'core-tests', or 'fuchsia'"
            ),
        }
    }

    /// The manifest name (as used in product TOMLs).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Petal => "petal",
            Self::CoreTests => "core-tests",
            Self::Fuchsia => "fuchsia",
        }
    }

    /// The image path for this manifest and architecture.
    pub fn image_path(&self, arch: Arch) -> PathBuf {
        match self {
            // The Fuchsia "image" is the imported ZBI, not a built
            // SFS image, so it lives in the prebuilt tree.
            Self::Fuchsia => fuchsia_prebuilt_dir(arch).join("fuchsia.zbi"),
            _ => PROJECT_DIR
                .join("target")
                .join("rootfs")
                .join("images")
                .join(format!("{}-{}.img", self.name(), arch.name())),
        }
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
            // Nothing to build: the artifacts are imported, not
            // generated. Validate them so failures surface here (with
            // the fix-it hint) instead of at kernel-build or boot time.
            Self::Fuchsia => {
                let dir = fuchsia_prebuilt_dir(arch);
                for file in ["userboot.elf", "vdso.elf", "fuchsia.zbi"] {
                    let path = dir.join(file);
                    assert!(
                        path.is_file(),
                        "fuchsia product requires {} — run `make fuchsia-import` \
                         (see docs/fuchsia.md; artifacts come from a Fuchsia build)",
                        path.display()
                    );
                }
                dir.join("fuchsia.zbi")
            }
        }
    }
}
