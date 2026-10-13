# Page Sizes

zCore uses a 4 KiB page size (`PAGE_SIZE = 4096`) on all platforms, matching
Fuchsia's Zircon kernel. This is a hard requirement for Fuchsia ABI
compatibility — `zx_system_get_page_size()` returns 4096, VMO sizes are
4K-aligned, and VMAR mappings use 4K granularity.

## Platform matrix

| Mode | Architecture | Host OS | Host page size | Guest page size | Notes |
|------|-------------|---------|----------------|-----------------|-------|
| Bare-metal | x86_64 | none | 4K | 4K | Native match |
| Bare-metal | aarch64 | none | 4K/16K/64K (configurable) | 4K | ARM MMU configured for 4K granule |
| Bare-metal | riscv64 | none | 4K | 4K | Native match |
| Libos | x86_64 | Linux | 4K | 4K | Native match — `MAP_SHARED` works directly |
| Libos | aarch64 | macOS | **16K** | 4K | **Mismatch** — see below |
| Libos | aarch64 | Linux | 4K | 4K | Native match (most ARM64 Linux distros use 4K) |

## macOS aarch64: 4K guest on 16K host

Apple Silicon Macs use 16 KiB host pages. The `mmap` syscall requires
file offsets to be 16K-aligned. This creates a problem for the libos
mock physical memory layer:

- zCore allocates 4K "physical frames" as offsets into a tempfile
- `MAP_SHARED` with `MAP_FIXED` maps a file region at a user VA
- On macOS, this only works if the file offset is 16K-aligned
- A 4K guest page at offset 0x1000 cannot be independently file-backed

### Current workaround (mock_mem.rs)

When file-backed `MAP_SHARED` cannot work (offset not 16K-aligned, or
conflicting guest pages within the same 16K host page), the code falls
back to `MAP_ANON` + `memcpy`. This creates an anonymous private page
at the user VA, disconnected from the tempfile-backed PMEM view.

To maintain coherence between the two views:
- `pmem_write` syncs data to anonymous user VA pages
- `pmem_read` syncs data from anonymous user VA pages to PMEM
- Reverse mapping (`paddr → vaddr`) tracks which pages are anonymous

### Limitations

- Performance: every VMO read/write on anonymous pages requires a memcpy sync
- Correctness: direct pointer writes through the user VA may not be
  visible via `pmem_read` until a sync point
- Test isolation: the global `MOCK_PHYS_MEM` state persists across tests

### Future: correct fix

The proper fix requires the libos frame allocator to be host-page-aware:
allocate 4K guest frames within 16K host pages such that the in-page
offset matches the target VA offset. This guarantees
`vaddr % 16K == paddr % 16K`, making file-backed `MAP_SHARED` work
for every mapping.

This is complex because guest frames are allocated during VMO commit
(before the target VA is known) and mapped later by the VMAR. The
allocation and mapping steps would need to be coordinated.

## ARM64 bare-metal: 4K granule

On bare-metal aarch64 (Pi 400, QEMU aarch64), zCore configures the
ARM MMU for 4K translation granule (`TCR_EL1.TG0 = 0b00`). This
matches Fuchsia and avoids any page size mismatch.

The ARMv8-A architecture supports 4K, 16K, and 64K granules. zCore
uses 4K because:
1. Fuchsia uses 4K — ABI compatibility requires it
2. The C++ core-tests-standalone binary assumes 4K pages
3. 4K pages provide finer-grained memory protection
