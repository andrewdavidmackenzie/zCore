//! Namespace tests.
//!
//! Ports of Fuchsia's namespace-related tests. Namespaces provide
//! a hierarchical directory structure for capability routing in
//! Fuchsia's component model. Not directly applicable to the Zircon
//! kernel layer -- there is no Namespace kernel object in zCore.
//!
//! These tests exercise the channel-based IPC that underlies namespace
//! operations and document the namespace gap.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::KernelObject;

extern crate alloc;

/// Create an "empty namespace" — a channel pair that would serve
/// as the namespace handle.
#[test]
fn create_empty() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();
    assert_ne!(ch0.id(), 0);
    assert_ne!(ch1.id(), 0);
    // An empty namespace is represented by a channel with no entries.
}

/// Add an entry to a namespace. Simulate by writing a "directory
/// entry" message to a channel.
#[test]
fn add_entry() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Simulate adding "/svc" entry.
    ch0.write(MessagePacket {
        data: b"/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let msg = ch1.read().unwrap();
    assert_eq!(&msg.data, b"/svc");
}

/// Look up an entry by sending a request message.
#[test]
fn lookup_entry() {
    let _ctx = TestContext::new();
    let (client, server) = Channel::create();

    // Client sends lookup request.
    client
        .write(MessagePacket {
            data: b"lookup:/svc/logger".to_vec(),
            handles: alloc::vec::Vec::new(),
        })
        .unwrap();

    // Server reads the request.
    let msg = server.read().unwrap();
    assert!(msg.data.starts_with(b"lookup:"));
}

/// Remove an entry. Simulate by sending a "remove" message.
#[test]
fn remove_entry() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    ch0.write(MessagePacket {
        data: b"remove:/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let msg = ch1.read().unwrap();
    assert_eq!(&msg.data, b"remove:/svc");
}

/// Invalid path: empty path. Channel write succeeds (validation
/// would be in the namespace server, not the channel).
#[test]
fn invalid_path() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Empty path — channel accepts it (validation is in the server).
    ch0.write(MessagePacket {
        data: alloc::vec::Vec::new(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let msg = ch1.read().unwrap();
    assert!(msg.data.is_empty());
}

/// Duplicate path: write two entries with the same path.
/// The namespace server would reject this — at channel level,
/// both messages are accepted.
#[test]
fn duplicate_path() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    ch0.write(MessagePacket {
        data: b"/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();
    ch0.write(MessagePacket {
        data: b"/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let msg1 = ch1.read().unwrap();
    let msg2 = ch1.read().unwrap();
    assert_eq!(&msg1.data, b"/svc");
    assert_eq!(&msg2.data, b"/svc");
}

/// Nested paths: /svc and /svc/fuchsia.logger.Log
#[test]
fn nested_paths() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    ch0.write(MessagePacket {
        data: b"/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();
    ch0.write(MessagePacket {
        data: b"/svc/fuchsia.logger.Log".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let msg1 = ch1.read().unwrap();
    let msg2 = ch1.read().unwrap();
    assert_eq!(&msg1.data, b"/svc");
    assert_eq!(&msg2.data, b"/svc/fuchsia.logger.Log");
}

/// Enumerate entries by reading all messages from the channel.
#[test]
fn enumerate_entries() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    let entries = [b"/svc".as_slice(), b"/dev", b"/boot"];
    for entry in &entries {
        ch0.write(MessagePacket {
            data: entry.to_vec(),
            handles: alloc::vec::Vec::new(),
        })
        .unwrap();
    }

    let mut received = alloc::vec::Vec::new();
    for _ in 0..3 {
        let msg = ch1.read().unwrap();
        received.push(msg.data);
    }
    assert_eq!(received.len(), 3);
}

/// Namespace lifecycle: create, populate, use, destroy.
#[test]
fn namespace_lifecycle() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Populate.
    ch0.write(MessagePacket {
        data: b"/svc".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    // Use.
    let msg = ch1.read().unwrap();
    assert_eq!(&msg.data, b"/svc");

    // Destroy (drop channels).
    drop(ch0);
    drop(ch1);
}

/// Transfer namespace handle to a child process. Simulate by
/// transferring a channel handle.
#[test]
fn transfer_namespace() {
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Add the channel as a handle in the process.
    let h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    assert_ne!(h, 0);

    // Write through the other end.
    ch1.write(MessagePacket {
        data: b"transferred".to_vec(),
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    // Read using the handle.
    let channel: alloc::sync::Arc<Channel> = ctx.proc.get_object(h).unwrap();
    let msg = channel.read().unwrap();
    assert_eq!(&msg.data, b"transferred");
}
