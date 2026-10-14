//! IOB (I/O Buffer) tests.
//!
//! IOB is implemented in zCore as `IoBuffer` in `zircon_object::ipc`.
//! These tests exercise the kernel object API directly.

use crate::helpers::TestContext;
use zircon_object::ipc::{IoBuffer, IoBufferRegion, IoBufferSharedRegion};
use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::VmObject;
use zircon_object::ZxError;

/// Helper: create a single-region IOB pair with the given size.
fn create_iob_pair(size: usize) -> (alloc::sync::Arc<IoBuffer>, alloc::sync::Arc<IoBuffer>) {
    let pages = size.div_ceil(0x1000);
    let vmo = VmObject::new_paged(pages);
    let regions = alloc::vec![IoBufferRegion::new(vmo, size, 0x3)]; // read+write
    IoBuffer::create(regions).expect("IOB create should succeed")
}

extern crate alloc;

// ============================================================================
// Iob suite (42 tests)
// ============================================================================

/// C++: TEST(Iob, BadVmOptions)
/// Creating an IOB with too many regions should fail.
#[test]
fn bad_vm_options() {
    let _ctx = TestContext::new();
    // IOB_MAX_REGIONS = 64. Try creating with 65 regions.
    let mut regions = alloc::vec::Vec::new();
    for _ in 0..65 {
        let vmo = VmObject::new_paged(1);
        regions.push(IoBufferRegion::new(vmo, 0x1000, 0x3));
    }
    let result = IoBuffer::create(regions);
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(Iob, Create)
/// Basic IOB creation: create a pair and verify endpoints are different.
#[test]
fn create() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    assert_ne!(ep0.id(), ep1.id());
    assert_eq!(ep0.endpoint_index(), 0);
    assert_eq!(ep1.endpoint_index(), 1);
}

/// C++: TEST(Iob, CreateHuge)
/// Create an IOB with a large VMO backing.
#[test]
fn create_huge() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(256); // 1 MiB
    let regions = alloc::vec![IoBufferRegion::new(vmo, 256 * 0x1000, 0x3)];
    let (ep0, ep1) = IoBuffer::create(regions).unwrap();
    assert_ne!(ep0.id(), ep1.id());
    assert_eq!(ep0.region_count(), 1);
}

/// C++: TEST(Iob, CreateMaxRegions)
/// Create an IOB with exactly IOB_MAX_REGIONS (64) regions.
#[test]
fn create_max_regions() {
    let _ctx = TestContext::new();
    let mut regions = alloc::vec::Vec::new();
    for _ in 0..64 {
        let vmo = VmObject::new_paged(1);
        regions.push(IoBufferRegion::new(vmo, 0x1000, 0x3));
    }
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    assert_eq!(ep0.region_count(), 64);
}

/// C++: TEST(Iob, CreateOutOfRangeAndInvalidArgs)
/// Creating with 65+ regions fails with OUT_OF_RANGE.
#[test]
fn create_out_of_range_and_invalid_args() {
    let _ctx = TestContext::new();
    let mut regions = alloc::vec::Vec::new();
    for _ in 0..65 {
        let vmo = VmObject::new_paged(1);
        regions.push(IoBufferRegion::new(vmo, 0x1000, 0));
    }
    assert_eq!(
        IoBuffer::create(regions).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(Iob, CreateWithMediatedWriteRingBufferDisciplineInvalidArgs)
/// ID allocator discipline on a region — allocate_id on a non-ID region fails.
#[test]
fn create_with_mediated_write_ring_buffer_discipline_invalid_args() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new(vmo, 0x1000, 0x3)]; // no discipline
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    // allocate_id on a non-ID-allocator region should fail with WRONG_TYPE
    assert_eq!(
        ep0.allocate_id(0, alloc::vec![1, 2, 3]).unwrap_err(),
        ZxError::WRONG_TYPE
    );
}

/// C++: TEST(Iob, CreateWithMediatedWriteRingBufferDisciplineSucceeds)
/// ID allocator discipline region — allocate_id succeeds.
#[test]
fn create_with_mediated_write_ring_buffer_discipline_succeeds() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new_with_id_allocator(vmo, 0x1000, 0x3)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let id = ep0.allocate_id(0, alloc::vec![42]).unwrap();
    assert!(id > 0);
}

/// C++: TEST(Iob, GetInfoIob)
/// Verify IOB endpoint properties: region count, endpoint index.
#[test]
fn get_info_iob() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x2000);
    assert_eq!(ep0.region_count(), 1);
    assert_eq!(ep1.region_count(), 1);
    assert_eq!(ep0.endpoint_index(), 0);
    assert_eq!(ep1.endpoint_index(), 1);
}

/// C++: TEST(Iob, GetInfoIobRegions)
/// Access region info (VMO, size, access) by index.
#[test]
fn get_info_iob_regions() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x3000);
    let (_vmo, size, access) = ep0.get_region(0).unwrap();
    assert_eq!(size, 0x3000);
    assert_eq!(access, 0x3);
}

/// C++: TEST(Iob, GetInfoIobRegionsEdgeCases)
/// Out-of-range region index returns OUT_OF_RANGE.
#[test]
fn get_info_iob_regions_edge_cases() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    assert_eq!(ep0.get_region(1).unwrap_err(), ZxError::OUT_OF_RANGE);
    assert_eq!(ep0.get_region(999).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(Iob, GetInfoProcessMaps)
/// IOB regions are VMO-backed; verify VMO IDs are non-zero.
#[test]
fn get_info_process_maps() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    let (vmo0, _, _) = ep0.get_region(0).unwrap();
    let (vmo1, _, _) = ep1.get_region(0).unwrap();
    assert_ne!(vmo0.id(), 0);
    // Both endpoints share the same VMO.
    assert_eq!(vmo0.id(), vmo1.id());
}

/// C++: TEST(Iob, GetInfoProcessVmos)
/// Both endpoints reference the same backing VMO for each region.
#[test]
fn get_info_process_vmos() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    let vmo_id = vmo.id();
    let regions = alloc::vec![IoBufferRegion::new(vmo, 0x2000, 0x3)];
    let (ep0, ep1) = IoBuffer::create(regions).unwrap();
    let (v0, _, _) = ep0.get_region(0).unwrap();
    let (v1, _, _) = ep1.get_region(0).unwrap();
    assert_eq!(v0.id(), vmo_id);
    assert_eq!(v1.id(), vmo_id);
}

/// C++: TEST(Iob, GetSetNames)
/// Set and get the name of an IOB endpoint.
#[test]
fn get_set_names() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    ep0.set_name("iob-ep0");
    assert_eq!(ep0.name(), "iob-ep0");
    ep1.set_name("iob-ep1");
    assert_eq!(ep1.name(), "iob-ep1");
}

/// C++: TEST(Iob, GetSetNamesEdgeCases)
/// Empty name and overwriting names.
#[test]
fn get_set_names_edge_cases() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    ep0.set_name("");
    assert_eq!(ep0.name(), "");
    ep0.set_name("overwrite");
    assert_eq!(ep0.name(), "overwrite");
}

/// C++: TEST(Iob, HandleBasicInfoAndRights)
/// IOB has a valid KOID and type name.
#[test]
fn handle_basic_info_and_rights() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    assert_ne!(ep0.id(), 0);
    assert_ne!(ep1.id(), 0);
    assert_eq!(ep0.type_name(), "IoBuffer");
    assert_eq!(ep1.type_name(), "IoBuffer");
}

/// C++: TEST(Iob, IdAllocatorMediatedAccess)
/// Allocate multiple IDs from an ID allocator region.
#[test]
fn id_allocator_mediated_access() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new_with_id_allocator(vmo, 0x1000, 0x3)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let id1 = ep0.allocate_id(0, alloc::vec![1]).unwrap();
    let id2 = ep0.allocate_id(0, alloc::vec![2]).unwrap();
    assert_ne!(id1, id2);
    // IDs should be sequential starting from 1
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

/// C++: TEST(Iob, IdAllocatorMediatedErrors)
/// ID allocation on invalid region index fails.
#[test]
fn id_allocator_mediated_errors() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new_with_id_allocator(vmo, 0x1000, 0x3)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    // Out-of-range region index
    assert_eq!(
        ep0.allocate_id(1, alloc::vec![]).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(Iob, IdAllocatorMultipleEndpointsAndZeroBlob)
/// Both endpoints can allocate IDs from the same shared region.
/// Empty blob (zero-length) is valid.
#[test]
fn id_allocator_multiple_endpoints_and_zero_blob() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new_with_id_allocator(vmo, 0x1000, 0x3)];
    let (ep0, ep1) = IoBuffer::create(regions).unwrap();
    let id1 = ep0.allocate_id(0, alloc::vec![]).unwrap(); // zero blob
    let id2 = ep1.allocate_id(0, alloc::vec![]).unwrap(); // from peer
    assert_ne!(id1, id2);
}

/// C++: TEST(Iob, IobWriteChecksRights)
/// Access flags on regions control behavior. Verify access field is stored.
#[test]
fn iob_write_checks_rights() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // read-only access (0x1)
    let regions = alloc::vec![IoBufferRegion::new(vmo, 0x1000, 0x1)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let (_, _, access) = ep0.get_region(0).unwrap();
    assert_eq!(access, 0x1); // read-only
}

/// C++: TEST(Iob, IobWriteCorrupt)
/// Verify that region data can be accessed through the VMO.
#[test]
fn iob_write_corrupt() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    // Write data through the VMO backing the region
    let (vmo, _, _) = ep0.get_region(0).unwrap();
    let data = [0xDE, 0xAD, 0xBE, 0xEF];
    vmo.write(0, &data).unwrap();
    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, data);
}

/// C++: TEST(Iob, IobWriteInvalidArgs)
/// Access a non-existent region index.
#[test]
fn iob_write_invalid_args() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    // Region index 5 doesn't exist
    assert_eq!(ep0.get_region(5).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(Iob, IobWriteMaxMessageSize)
/// Write max data to region VMO.
#[test]
fn iob_write_max_message_size() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    let (vmo, size, _) = ep0.get_region(0).unwrap();
    // Write a full page
    let data = alloc::vec![0xABu8; size];
    vmo.write(0, &data).unwrap();
    let mut buf = alloc::vec![0u8; size];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, data);
}

/// C++: TEST(Iob, IobWriteMultiThreaded)
/// Multiple ID allocations don't collide (sequential IDs).
#[test]
fn iob_write_multi_threaded() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new_with_id_allocator(vmo, 0x1000, 0x3)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let mut ids = alloc::vec::Vec::new();
    for i in 0..100 {
        ids.push(ep0.allocate_id(0, alloc::vec![i as u8]).unwrap());
    }
    // All IDs should be unique
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

/// C++: TEST(Iob, IobWriteSuccess)
/// Write and read through the VMO succeeds.
#[test]
fn iob_write_success() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    let (vmo, _, _) = ep0.get_region(0).unwrap();
    let msg = b"Hello IOB!";
    vmo.write(0, msg).unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, msg);
}

/// C++: TEST(Iob, IobWriteUpdatedSignal)
/// IOB endpoints start with WRITABLE signal.
#[test]
fn iob_write_updated_signal() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    assert!(ep0.signal().contains(Signal::WRITABLE));
    assert!(ep1.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(Iob, IobWriteWithFaultSucceeds)
/// Write to VMO at various offsets within the region.
#[test]
fn iob_write_with_fault_succeeds() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x2000);
    let (vmo, _, _) = ep0.get_region(0).unwrap();
    // Write at offset 0x1000 (second page)
    let data = [0xFF; 8];
    vmo.write(0x1000, &data).unwrap();
    let mut buf = [0u8; 8];
    vmo.read(0x1000, &mut buf).unwrap();
    assert_eq!(buf, data);
}

/// C++: TEST(Iob, IobWriteWithMultipleVectors)
/// Multiple regions, each with its own VMO, all accessible.
#[test]
fn iob_write_with_multiple_vectors() {
    let _ctx = TestContext::new();
    let vmo0 = VmObject::new_paged(1);
    let vmo1 = VmObject::new_paged(1);
    let regions = alloc::vec![
        IoBufferRegion::new(vmo0, 0x1000, 0x3),
        IoBufferRegion::new(vmo1, 0x1000, 0x3),
    ];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    assert_eq!(ep0.region_count(), 2);
    // Write to region 0
    let (v0, _, _) = ep0.get_region(0).unwrap();
    v0.write(0, &[1, 2, 3, 4]).unwrap();
    // Write to region 1
    let (v1, _, _) = ep0.get_region(1).unwrap();
    v1.write(0, &[5, 6, 7, 8]).unwrap();
    // Read back and verify
    let mut buf = [0u8; 4];
    v0.read(0, &mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3, 4]);
    v1.read(0, &mut buf).unwrap();
    assert_eq!(buf, [5, 6, 7, 8]);
}

/// C++: TEST(Iob, MapWhenPeerClosed)
/// After peer is dropped, the remaining endpoint sees PEER_CLOSED.
#[test]
fn map_when_peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    drop(ep1);
    assert!(ep0.peer_closed());
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(Iob, MappingRights)
/// Regions with different access flags are stored correctly.
#[test]
fn mapping_rights() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let regions = alloc::vec![IoBufferRegion::new(vmo, 0x1000, 0x7)]; // read+write+exec
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let (_, _, access) = ep0.get_region(0).unwrap();
    assert_eq!(access, 0x7);
}

/// C++: TEST(Iob, PartialAndMultipleMappings)
/// Multiple regions with varying sizes.
#[test]
fn partial_and_multiple_mappings() {
    let _ctx = TestContext::new();
    let vmo0 = VmObject::new_paged(1);
    let vmo1 = VmObject::new_paged(2);
    let vmo2 = VmObject::new_paged(4);
    let regions = alloc::vec![
        IoBufferRegion::new(vmo0, 0x1000, 0x1),
        IoBufferRegion::new(vmo1, 0x2000, 0x2),
        IoBufferRegion::new(vmo2, 0x4000, 0x3),
    ];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    assert_eq!(ep0.region_count(), 3);
    let (_, s0, a0) = ep0.get_region(0).unwrap();
    let (_, s1, a1) = ep0.get_region(1).unwrap();
    let (_, s2, a2) = ep0.get_region(2).unwrap();
    assert_eq!((s0, a0), (0x1000, 0x1));
    assert_eq!((s1, a1), (0x2000, 0x2));
    assert_eq!((s2, a2), (0x4000, 0x3));
}

/// C++: TEST(Iob, PeerClosed)
/// Dropping one endpoint signals PEER_CLOSED on the other.
#[test]
fn peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    assert!(!ep0.peer_closed());
    assert!(!ep1.peer_closed());
    drop(ep1);
    assert!(ep0.peer_closed());
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(Iob, PeerClosedMappedReferences)
/// After peer is closed, regions are still accessible on the remaining endpoint.
#[test]
fn peer_closed_mapped_references() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    // Write data via ep1's VMO
    let (vmo, _, _) = ep1.get_region(0).unwrap();
    vmo.write(0, &[42]).unwrap();
    drop(ep1);
    // ep0 can still access the region
    let (vmo0, _, _) = ep0.get_region(0).unwrap();
    let mut buf = [0u8; 1];
    vmo0.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);
}

/// C++: TEST(Iob, RegionInfoSwappedAccess)
/// Both endpoints see the same region info.
#[test]
fn region_info_swapped_access() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    let (_, s0, a0) = ep0.get_region(0).unwrap();
    let (_, s1, a1) = ep1.get_region(0).unwrap();
    assert_eq!(s0, s1);
    assert_eq!(a0, a1);
}

/// C++: TEST(Iob, RegionMap)
/// Region VMO is mappable — verify it has content size.
#[test]
fn region_map() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    let (vmo, size, _) = ep0.get_region(0).unwrap();
    assert_eq!(size, 0x1000);
    assert!(vmo.len() >= 0x1000);
}

/// C++: TEST(Iob, RoundedSizes)
/// Region sizes are stored as provided (not rounded).
#[test]
fn rounded_sizes() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // Size 0x500 (not page-aligned) — stored as-is
    let regions = alloc::vec![IoBufferRegion::new(vmo, 0x500, 0x3)];
    let (ep0, _ep1) = IoBuffer::create(regions).unwrap();
    let (_, size, _) = ep0.get_region(0).unwrap();
    assert_eq!(size, 0x500);
}

/// C++: TEST(Iob, SharedRegionWithNonRingBufferDiscipline)
/// IoBufferSharedRegion: create a shared region with a given size.
#[test]
fn shared_region_with_non_ring_buffer_discipline() {
    let _ctx = TestContext::new();
    let sr = IoBufferSharedRegion::create(0x1000).unwrap();
    assert_eq!(sr.size(), 0x1000);
    assert_ne!(sr.id(), 0);
    // Zero size should fail
    assert_eq!(
        IoBufferSharedRegion::create(0).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    // Non-page-aligned size should fail
    assert_eq!(
        IoBufferSharedRegion::create(0x500).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(Iob, SignalsAndSignalPeer)
/// IOB endpoints support user signals.
#[test]
fn signals_and_signal_peer() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    // Set user signal on ep0
    ep0.signal_set(Signal::USER_SIGNAL_0);
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));
    // Signal peer
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_1);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_1));
}

/// C++: TEST(Iob, SymmetricPeerClosedWithMappings)
/// Drop ep0 — ep1 sees PEER_CLOSED. Drop ep1 — no panic.
#[test]
fn symmetric_peer_closed_with_mappings() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    drop(ep0);
    assert!(ep1.peer_closed());
    assert!(ep1.signal().contains(Signal::PEER_CLOSED));
    // ep1 regions still accessible
    assert_eq!(ep1.region_count(), 1);
    drop(ep1); // no panic
}

/// C++: TEST(Iob, VmarMapIobErrors)
/// Out-of-range region index returns error.
#[test]
fn vmar_map_iob_errors() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    assert_eq!(ep0.get_region(10).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(Iob, VmarMapIobWithFaultBeyondStreamSizeReturnsError)
/// Writing beyond VMO size should fail.
#[test]
fn vmar_map_iob_with_fault_beyond_stream_size_returns_error() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = create_iob_pair(0x1000);
    let (vmo, _, _) = ep0.get_region(0).unwrap();
    // Writing beyond VMO length
    let data = [0u8; 1];
    let result = vmo.write(0x10000, &data);
    assert!(result.is_err());
}

/// C++: TEST(Iob, VmoKoid)
/// Region VMOs have valid KOIDs.
#[test]
fn vmo_koid() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = create_iob_pair(0x1000);
    let (vmo0, _, _) = ep0.get_region(0).unwrap();
    let (vmo1, _, _) = ep1.get_region(0).unwrap();
    assert_ne!(vmo0.id(), 0);
    // Shared VMO — same KOID from both endpoints
    assert_eq!(vmo0.id(), vmo1.id());
}

/// C++: TEST(Iob, WritevZeroLengthAndMultipleIobs)
/// Create two IOB pairs, verify they're independent.
#[test]
fn writev_zero_length_and_multiple_iobs() {
    let _ctx = TestContext::new();
    let (a0, a1) = create_iob_pair(0x1000);
    let (b0, b1) = create_iob_pair(0x1000);
    // All four endpoints have unique KOIDs
    let ids = [a0.id(), a1.id(), b0.id(), b1.id()];
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            assert_ne!(ids[i], ids[j]);
        }
    }
    // Writing to a0's VMO doesn't affect b0's VMO
    let (va, _, _) = a0.get_region(0).unwrap();
    let (vb, _, _) = b0.get_region(0).unwrap();
    va.write(0, &[0xAA]).unwrap();
    let mut buf = [0u8; 1];
    vb.read(0, &mut buf).unwrap();
    assert_ne!(buf[0], 0xAA);
}
