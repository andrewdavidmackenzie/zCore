# Fuchsia userspace artifacts (x86_64)

Imported by `make fuchsia-import` (tools/scripts/fuchsia-import.sh)
from a Fuchsia build output. Do not edit by hand.

- Fuchsia revision: e1af0ce0efad702848b17a0fc85e5406f94a82ee (2026-10-05 [flatland] 160-4: Persistent SceneState & reuse)
- Build directory: `/home/amackenz/RustroverProjects/zCore/../fuchsia/fuchsia/fuchsia/out/bringup.x64-balanced`
- Boot image: `fuchsia.zbi` (kernel item unused by zCore; userboot
  and the vDSO are carved from its kernel package)

| File | Source (kernel package bootfs) | Size | sha256 |
|------|-------------------------------|------|--------|
| `userboot.elf` | `zircon/userboot` | 900392 | `c3bc239ba8716f1a46cd80f2d2080e7588887d1cc6a053d2f6e963dbbbccf200` |
| `userboot-test-rust.elf` | `zircon/userboot-test-rust` | 920872 | `27e0c10d5dd44e6c2cef6e47651bc1a5cd77eb27aa7ddb65229dc93148a0f504` |
| `vdso.elf` | `zircon/vdso` | 57344 | `3bd1e2035e4e9c40f914b6765e150b4a4ee8ae28fd7b1145efe11d7b491df25a` |
| `fuchsia.zbi` | `copied from images.json zircon-a` | 30232944 | `1ca2fcee32636277337c2008f960c9f7e7e25103367d676c92f4c47853dbb719` |
