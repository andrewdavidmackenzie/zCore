//! Fuchsia loader service (FIDL `fuchsia.ldsvc/Loader`).
//!
//! Serves `LOAD_OBJECT` requests from `ld.so.1` by looking up shared
//! libraries in the rootfs at `/lib/<name>` and returning them as VMOs.
//!
//! Only needed when running dynamically-linked Fuchsia binaries.

use alloc::format;
use alloc::sync::Arc;
use alloc::vec;

use crate::ipc::Channel;
use crate::object::{Handle, KernelObject, Rights, Signal};
use crate::vm::VmObject;

use super::read_rootfs_file;

/// Spawn a kernel task that serves the Fuchsia loader service protocol.
///
/// `ld.so.1` sends `LOADER_SVC_OP_LOAD_OBJECT` (opcode 2) requests
/// on the channel with a library name. We look up the library in
/// the rootfs at `/lib/<name>`, create a VMO from the file data,
/// and send it back as a handle.
pub(crate) fn spawn_loader_service(channel: Arc<Channel>) {
    // The loader service runs as a synchronous loop on a kernel task.
    // It exits when the channel is closed (ld.so.1 drops its end
    // after finishing loading).
    hal_impl::thread::spawn(async move {
        info!("loader_service: started");
        loop {
            // Wait for a message from ld.so.1.
            let object: Arc<dyn KernelObject> = channel.clone();
            object
                .wait_signal(Signal::READABLE | Signal::PEER_CLOSED)
                .await;

            // Check if the channel was closed.
            if channel.signal().contains(Signal::PEER_CLOSED) {
                info!("loader_service: channel closed, exiting");
                break;
            }

            // Read the request.
            let msg = match channel.read() {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("loader_service: read failed: {:?}", e);
                    break;
                }
            };

            // Parse FIDL loader service message.
            // Header: txid(u32) + flags(3 bytes) + magic(1 byte) + ordinal(u64) = 16 bytes
            // Payload for LOAD_OBJECT: fidl_string_t(size:u64, data:u64) + string bytes
            if msg.data.len() < 16 {
                warn!("loader_service: message too short ({})", msg.data.len());
                continue;
            }
            let txid = u32::from_le_bytes(msg.data[0..4].try_into().unwrap());
            let ordinal = u64::from_le_bytes(msg.data[8..16].try_into().unwrap());

            const LDMSG_OP_LOAD_OBJECT: u64 = 0x48C5_A151_D6DF_2853;
            const LDMSG_OP_DONE: u64 = 0x63BA_6B76_D367_1001;
            const LDMSG_OP_CONFIG: u64 = 0x6A8A_1A14_6463_2841;

            match ordinal {
                LDMSG_OP_LOAD_OBJECT => {
                    // Payload: fidl_string_t at offset 16
                    if msg.data.len() < 32 {
                        warn!("loader_service: LOAD_OBJECT too short");
                        continue;
                    }
                    let str_size =
                        u64::from_le_bytes(msg.data[16..24].try_into().unwrap()) as usize;
                    // String data starts at offset 32 (after fidl_string_t)
                    let name_bytes = if msg.data.len() >= 32 + str_size {
                        &msg.data[32..32 + str_size]
                    } else {
                        &msg.data[32..]
                    };
                    // Trim trailing NUL if present.
                    let name_end = name_bytes
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(name_bytes.len());
                    let name = core::str::from_utf8(&name_bytes[..name_end]).unwrap_or("?");
                    info!("loader_service: LOAD_OBJECT '{}'", name);

                    // Look up the library in the rootfs.
                    let lib_path = format!("/lib/{}", name);
                    let response = if let Some(file_data) = read_rootfs_file(&lib_path) {
                        info!(
                            "loader_service: found '{}' ({} bytes)",
                            lib_path,
                            file_data.len()
                        );
                        let vmo = VmObject::new_paged(crate::vm::pages(file_data.len()));
                        if let Err(e) = vmo.write(0, &file_data) {
                            warn!("loader_service: VMO write failed: {:?}", e);
                            // ZX_ERR_NO_MEMORY (-4)
                            make_ldmsg_response(txid, ordinal, -4i32, vec![])
                        } else {
                            vmo.set_name(name);
                            make_ldmsg_response(
                                txid,
                                ordinal,
                                0i32, // ZX_OK
                                vec![Handle::new(vmo, Rights::DEFAULT_VMO | Rights::EXECUTE)],
                            )
                        }
                    } else {
                        warn!("loader_service: '{}' not found in rootfs", lib_path);
                        // ZX_ERR_NOT_FOUND (-25)
                        make_ldmsg_response(txid, ordinal, -25i32, vec![])
                    };

                    if let Err(e) = channel.write(response) {
                        warn!("loader_service: write response failed: {:?}", e);
                        break;
                    }
                }
                LDMSG_OP_CONFIG => {
                    info!("loader_service: CONFIG (ignored)");
                    let response = make_ldmsg_response(txid, ordinal, 0i32, vec![]);
                    channel.write(response).ok();
                }
                LDMSG_OP_DONE => {
                    info!("loader_service: DONE");
                    break;
                }
                _ => {
                    warn!("loader_service: unknown ordinal {:#x}", ordinal);
                    let response = make_ldmsg_response(txid, ordinal, -2i32, vec![]); // NOT_SUPPORTED
                    channel.write(response).ok();
                }
            }
        }
        info!("loader_service: done");
    });
}

/// FIDL message header. Must match `fidl_message_header_t` from `zircon/fidl.h`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FidlMessageHeader {
    txid: u32,
    at_rest_flags: [u8; 2],
    dynamic_flags: u8,
    magic_number: u8,
    ordinal: u64,
}

const _: () = assert!(core::mem::size_of::<FidlMessageHeader>() == 16);

/// FIDL loader service response. Must match `ldmsg_rsp_t` from `ldmsg/ldmsg.h`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LdmsgResponse {
    header: FidlMessageHeader,
    rv: i32,
    /// FIDL_HANDLE_PRESENT (0xFFFFFFFF) or FIDL_HANDLE_ABSENT (0)
    object: u32,
}

const _: () = assert!(core::mem::size_of::<LdmsgResponse>() == 24);

/// Construct a FIDL loader service response message.
fn make_ldmsg_response(
    txid: u32,
    ordinal: u64,
    status: i32,
    handles: alloc::vec::Vec<Handle>,
) -> crate::ipc::MessagePacket {
    let mut data = alloc::vec::Vec::with_capacity(core::mem::size_of::<LdmsgResponse>());
    // fidl_message_header_t
    data.extend_from_slice(&txid.to_le_bytes());
    data.extend_from_slice(&[0x02, 0x00]); // at_rest_flags (USE_VERSION_V2)
    data.push(0x00); // dynamic_flags
    data.push(0x01); // magic_number (kFidlWireFormatMagicNumberInitial)
    data.extend_from_slice(&ordinal.to_le_bytes());
    // rv (status)
    data.extend_from_slice(&status.to_le_bytes());
    // handle present/absent marker
    let marker: u32 = if handles.is_empty() { 0 } else { 0xFFFF_FFFF };
    data.extend_from_slice(&marker.to_le_bytes());
    debug_assert_eq!(data.len(), core::mem::size_of::<LdmsgResponse>());
    crate::ipc::MessagePacket { data, handles }
}
