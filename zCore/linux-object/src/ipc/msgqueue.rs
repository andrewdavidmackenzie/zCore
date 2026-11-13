//! System V Message Queue implementation.
//!
//! Follows the same pattern as `semary.rs` (semaphore arrays):
//! a global key→queue map with `Weak` references, plus per-process
//! `Arc` references for lifetime management.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use lock::{Mutex, RwLock};

use super::IpcPerm;

/// Maximum bytes in a message queue (MSGMNB).
const MSGMNB: usize = 16384;

/// Maximum size of a single message (MSGMAX).
const MSGMAX: usize = 8192;

/// A single message in the queue.
#[derive(Clone)]
pub struct MsgEntry {
    /// Message type (must be > 0).
    pub mtype: i64,
    /// Message data.
    pub mtext: Vec<u8>,
}

/// Internal state of a message queue, protected by a Mutex.
pub struct MsqidDsInner {
    /// IPC permissions and key.
    pub perm: IpcPerm,
    /// Queued messages.
    pub messages: VecDeque<MsgEntry>,
    /// Current total bytes in queue.
    pub cbytes: usize,
    /// Current number of messages.
    pub qnum: usize,
    /// Maximum bytes allowed.
    pub qbytes: usize,
    /// PID of last msgsnd.
    pub lspid: u32,
    /// PID of last msgrcv.
    pub lrpid: u32,
    /// Last msgsnd time.
    pub stime: usize,
    /// Last msgrcv time.
    pub rtime: usize,
    /// Last change time.
    pub ctime: usize,
}

/// A System V message queue.
pub struct MsgQueue {
    inner: Mutex<MsqidDsInner>,
}

/// Global map from IPC key to message queue.
static KEY2MSG: spin::Lazy<RwLock<BTreeMap<u32, Weak<MsgQueue>>>> =
    spin::Lazy::new(|| RwLock::new(BTreeMap::new()));

impl MsgQueue {
    /// Create a new message queue with the given key and permissions.
    fn new(key: u32, mode: u32, uid: u32, gid: u32) -> Self {
        MsgQueue {
            inner: Mutex::new(MsqidDsInner {
                perm: IpcPerm {
                    key,
                    uid,
                    gid,
                    cuid: uid,
                    cgid: gid,
                    mode: mode & 0x1ff,
                    __seq: 0,
                    __pad1: 0,
                    __pad2: 0,
                },
                messages: VecDeque::new(),
                cbytes: 0,
                qnum: 0,
                qbytes: MSGMNB,
                lspid: 0,
                lrpid: 0,
                stime: 0,
                rtime: 0,
                ctime: 0,
            }),
        }
    }

    /// Get or create a message queue by key (follows semget pattern).
    pub fn get_or_create(
        key: u32,
        flags: usize,
        uid: u32,
        gid: u32,
    ) -> Result<Arc<Self>, crate::error::LxError> {
        use super::IpcGetFlag;
        use crate::error::LxError;
        let ipc_flags = IpcGetFlag::from_bits_truncate(flags);
        let mode = flags & 0x1ff;

        let mut map = KEY2MSG.write();

        // IPC_PRIVATE: always create a new queue
        let actual_key = if key == 0 {
            (1u32..).find(|k| !map.contains_key(k)).unwrap()
        } else {
            key
        };

        // Check if queue exists
        if let Some(weak) = map.get(&actual_key) {
            if let Some(existing) = weak.upgrade() {
                if ipc_flags.contains(IpcGetFlag::CREAT)
                    && ipc_flags.contains(IpcGetFlag::EXCLUSIVE)
                {
                    return Err(LxError::EEXIST);
                }
                return Ok(existing);
            }
            // Weak expired — remove stale entry
            map.remove(&actual_key);
        }

        // Not found — must have CREAT flag (unless key == 0)
        if key != 0 && !ipc_flags.contains(IpcGetFlag::CREAT) {
            return Err(LxError::ENOENT);
        }

        let queue = Arc::new(MsgQueue::new(actual_key, mode as u32, uid, gid));
        map.insert(actual_key, Arc::downgrade(&queue));
        Ok(queue)
    }

    /// Send a message to the queue.
    pub fn send(&self, mtype: i64, mtext: Vec<u8>, pid: u32) -> Result<(), crate::error::LxError> {
        use crate::error::LxError;
        if mtype <= 0 {
            return Err(LxError::EINVAL);
        }
        if mtext.len() > MSGMAX {
            return Err(LxError::EINVAL);
        }
        let mut inner = self.inner.lock();
        if inner.cbytes + mtext.len() > inner.qbytes {
            return Err(LxError::EAGAIN); // queue full (IPC_NOWAIT behavior)
        }
        let len = mtext.len();
        inner.messages.push_back(MsgEntry { mtype, mtext });
        inner.cbytes += len;
        inner.qnum += 1;
        inner.lspid = pid;
        inner.stime = 0; // TODO: use real time
        Ok(())
    }

    /// Receive a message from the queue.
    ///
    /// `msgtyp`: 0 = first message, >0 = first of that type, <0 = first with type <= |msgtyp|
    /// `msgsz`: maximum data size to receive
    /// `msg_noerror`: if true, truncate oversized messages instead of returning EINVAL
    pub fn recv(
        &self,
        msgtyp: i64,
        msgsz: usize,
        msg_noerror: bool,
        pid: u32,
    ) -> Result<MsgEntry, crate::error::LxError> {
        use crate::error::LxError;
        let mut inner = self.inner.lock();

        // Find matching message
        let idx = if msgtyp == 0 {
            // First message
            if inner.messages.is_empty() {
                return Err(LxError::EAGAIN);
            }
            0
        } else if msgtyp > 0 {
            // First message of the given type
            inner
                .messages
                .iter()
                .position(|m| m.mtype == msgtyp)
                .ok_or(LxError::EAGAIN)?
        } else {
            // First message with type <= |msgtyp|
            let abs_type = -msgtyp;
            inner
                .messages
                .iter()
                .position(|m| m.mtype <= abs_type)
                .ok_or(LxError::EAGAIN)?
        };

        let msg = &inner.messages[idx];
        if msg.mtext.len() > msgsz && !msg_noerror {
            return Err(LxError::EINVAL);
        }

        let mut msg = inner.messages.remove(idx).unwrap();
        inner.cbytes -= msg.mtext.len();
        inner.qnum -= 1;
        inner.lrpid = pid;
        inner.rtime = 0; // TODO: use real time

        // Truncate if needed
        if msg.mtext.len() > msgsz {
            msg.mtext.truncate(msgsz);
        }

        Ok(msg)
    }

    /// Get queue metadata for IPC_STAT.
    pub fn stat(&self) -> MsqidDs {
        let inner = self.inner.lock();
        MsqidDs {
            perm: inner.perm,
            stime: inner.stime,
            rtime: inner.rtime,
            ctime: inner.ctime,
            cbytes: inner.cbytes,
            qnum: inner.qnum,
            qbytes: inner.qbytes,
            lspid: inner.lspid,
            lrpid: inner.lrpid,
            _pad: [0; 4],
        }
    }

    /// Update queue settings for IPC_SET.
    pub fn set(&self, perm_mode: u32, qbytes: usize) {
        let mut inner = self.inner.lock();
        inner.perm.mode = perm_mode & 0x1ff;
        inner.qbytes = qbytes;
    }
}

/// The `msqid_ds` structure returned by IPC_STAT.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MsqidDs {
    /// IPC permissions.
    pub perm: IpcPerm,
    /// Last msgsnd time.
    pub stime: usize,
    /// Last msgrcv time.
    pub rtime: usize,
    /// Last change time.
    pub ctime: usize,
    /// Current bytes in queue.
    pub cbytes: usize,
    /// Current number of messages.
    pub qnum: usize,
    /// Maximum bytes allowed.
    pub qbytes: usize,
    /// PID of last msgsnd.
    pub lspid: u32,
    /// PID of last msgrcv.
    pub lrpid: u32,
    /// Padding.
    pub _pad: [usize; 4],
}

/// Message queue table in a process.
#[derive(Default, Clone)]
pub struct MsgProc {
    queues: BTreeMap<MsgId, Arc<MsgQueue>>,
}

/// Message queue identifier (in a process).
type MsgId = usize;

impl MsgProc {
    /// Insert a queue and return its ID.
    pub fn add(&mut self, queue: Arc<MsgQueue>) -> MsgId {
        let id = self.get_free_id();
        self.queues.insert(id, queue);
        id
    }

    /// Get a queue by ID.
    pub fn get(&self, id: MsgId) -> Option<Arc<MsgQueue>> {
        self.queues.get(&id).cloned()
    }

    /// Remove a queue by ID.
    pub fn remove(&mut self, id: MsgId) {
        self.queues.remove(&id);
    }

    /// Get a free ID.
    fn get_free_id(&self) -> MsgId {
        (0..).find(|i| !self.queues.contains_key(i)).unwrap()
    }
}
