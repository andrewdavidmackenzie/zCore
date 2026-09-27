//! Objects for IPC.

mod channel;
mod fifo;
mod iob;
mod socket;

pub use self::{channel::*, fifo::*, iob::*, socket::*};
