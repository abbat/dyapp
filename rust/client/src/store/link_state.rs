//! The owner's local state a linked device starts from: blocks, mutes, favorites, names and verified marks,
//! likes, passes and whether the profile is hidden. Later changes reach it as own-device sync.
use super::likes::{load_hlc, save_hlc};
use super::{Error, Store};

/// The `settings` row of a record's state until the linked device has listed itself.
const PENDING: &str = "link_pending";
/// The `settings` row of a device that joined by a link. It outlives the link: chats here start
/// at that moment, for good.
const LINKED: &str = "linked_device";

#[cfg(test)]
#[path = "link_state/retry_tests.rs"]
mod retry_tests;

#[derive(Clone, PartialEq, prost::Message)]
struct LinkState {
    #[prost(message, repeated, tag = "1")]
    peers: Vec<PeerSnapshot>,
    #[prost(message, repeated, tag = "2")]
    likes: Vec<Like>,
    #[prost(message, repeated, tag = "3")]
    passes: Vec<Pass>,
    #[prost(uint64, tag = "4")]
    hlc: u64,
    #[prost(bool, tag = "5")]
    hidden: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent peer settings retain their separate protobuf tags and merge clocks"
)]
struct PeerSnapshot {
    #[prost(string, tag = "1")]
    peer_id: String,
    #[prost(bool, tag = "2")]
    blocked: bool,
    #[prost(bool, tag = "3")]
    muted: bool,
    #[prost(string, tag = "4")]
    local_name: String,
    #[prost(bytes = "vec", optional, tag = "5")]
    seen_devices: Option<Vec<u8>>,
    #[prost(int64, optional, tag = "6")]
    seen_version: Option<i64>,
    #[prost(bool, tag = "7")]
    verified: bool,
    #[prost(bool, tag = "8")]
    new_device: bool,
    #[prost(int64, tag = "9")]
    blocked_hlc: i64,
    #[prost(int64, tag = "10")]
    muted_hlc: i64,
    #[prost(int64, tag = "11")]
    name_hlc: i64,
    #[prost(bool, tag = "12")]
    favorite: bool,
    #[prost(int64, tag = "13")]
    favorite_hlc: i64,
    #[prost(bytes = "vec", tag = "14")]
    blocked_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "15")]
    muted_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "16")]
    name_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "17")]
    favorite_device: Vec<u8>,
    #[prost(string, tag = "18")]
    note: String,
    #[prost(int64, tag = "19")]
    note_hlc: i64,
    #[prost(bytes = "vec", tag = "20")]
    note_device: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Like {
    #[prost(string, tag = "1")]
    sender: String,
    #[prost(string, tag = "2")]
    target: String,
    #[prost(bool, tag = "3")]
    liked: bool,
    #[prost(int64, tag = "4")]
    hlc: i64,
    #[prost(bytes = "vec", tag = "5")]
    device: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    event_id: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Pass {
    #[prost(string, tag = "1")]
    peer_id: String,
    #[prost(bool, tag = "2")]
    passed: bool,
    #[prost(int64, tag = "3")]
    updated_hlc: i64,
    #[prost(bytes = "vec", tag = "4")]
    updated_device: Vec<u8>,
}

#[path = "link_state/export.rs"]
mod export;
#[path = "link_state/import.rs"]
mod import;
#[path = "link_state/pending.rs"]
mod pending;
#[path = "link_state/progress.rs"]
mod progress;
#[cfg(test)]
#[path = "link_state/tests.rs"]
mod tests;
