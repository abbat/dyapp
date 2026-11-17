use super::*;
use crate::chat::{AttachmentMessage, Body};
use crate::store::messages::VerifiedChatSender;
use crate::store::tests::catalog_group;
use dyapp_identity::Identity;

mod cache;

const ID: [u8; 16] = [7; 16];
const FIRST: &[u8] = b"first";
const SECOND: &[u8] = b"second";

struct Fixture {
    store: Store,
    peer: String,
    first: Attachment,
    second: Attachment,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_bytes(b"first original", b"second original")
    }

    fn with_bytes(first_bytes: &[u8], second_bytes: &[u8]) -> Self {
        let owner = Identity::try_generate().unwrap();
        let peer = Identity::try_generate().unwrap().peer_id();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/core-photo-lookup")
            .join(owner.peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(&dir.join("client.db"), &owner).unwrap();
        let first = record(&mut store, &peer, FIRST, first_bytes);
        let second = record(&mut store, &peer, SECOND, second_bytes);
        Self {
            store,
            peer,
            first,
            second,
            dir,
        }
    }
}

fn record(store: &mut Store, peer: &str, group: &[u8], bytes: &[u8]) -> Attachment {
    catalog_group(store, group, peer);
    let (attachment, _) = crate::attachment::seal(bytes).unwrap();
    let body = Body::Attachment(AttachmentMessage {
        id: ID,
        hlc: 1000,
        attachment: attachment.clone(),
    });
    // Descriptor selection begins at already-verified stored sender metadata.
    assert!(store
        .record_chat_attachment(
            VerifiedChatSender {
                group,
                author: peer,
                device: &[3; 32]
            },
            &body,
            true,
        )
        .unwrap());
    attachment
}

#[test]
fn same_author_and_logical_id_in_distinct_groups_select_their_own_descriptor() {
    let f = Fixture::new();
    let first = stored(&f.store, &f.peer, FIRST, &f.peer, &ID).unwrap();
    let second = stored(&f.store, &f.peer, SECOND, &f.peer, &ID).unwrap();
    assert_eq!(first, f.first);
    assert_eq!(second, f.second);
    assert_ne!(first.plaintext_sha256, second.plaintext_sha256);
    assert_eq!(f.store.current_chat_attachments(&f.peer).unwrap().len(), 2);
    assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
}

#[test]
fn partial_stale_or_other_peer_selectors_do_not_fall_back_to_the_first_photo() {
    let f = Fixture::new();
    let not_found = |peer: &str, group: &[u8], author: &str, id: &[u8]| {
        matches!(
            stored(&f.store, peer, group, author, id),
            Err(LookupError::NotFound)
        )
    };
    assert!(not_found(&f.peer, b"absent", &f.peer, &ID));
    assert!(not_found(&f.peer, &[], &f.peer, &ID));
    assert!(not_found(&f.peer, FIRST, "other", &ID));
    assert!(not_found(&f.peer, FIRST, &f.peer, &[]));
    let mut changed_id = ID;
    changed_id[0] ^= 1;
    assert!(not_found(&f.peer, FIRST, &f.peer, &changed_id));
    assert!(not_found("other", FIRST, &f.peer, &ID));
}

#[test]
fn block_precedes_malformed_selection_and_clear_keeps_the_other_group() {
    let mut f = Fixture::new();
    f.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(
        stored(&f.store, &f.peer, &[], "", &[]),
        Err(LookupError::Blocked)
    ));
    assert!(matches!(
        stored(&f.store, &f.peer, FIRST, &f.peer, &ID),
        Err(LookupError::Blocked)
    ));
    f.store.set_blocked(&f.peer, false).unwrap();
    f.store.clear_chat(FIRST).unwrap();
    assert!(matches!(
        stored(&f.store, &f.peer, FIRST, &f.peer, &ID),
        Err(LookupError::NotFound)
    ));
    assert_eq!(
        stored(&f.store, &f.peer, SECOND, &f.peer, &ID).unwrap(),
        f.second
    );
}

#[test]
fn invalid_owner_queries_preserve_the_store_failure_category() {
    let owner = Identity::try_generate().unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/core-photo-lookup")
        .join(owner.peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("client.db"), &owner).unwrap();
    assert!(matches!(
        stored(&store, &owner.peer_id(), FIRST, &owner.peer_id(), &ID),
        Err(LookupError::Store(store::Error::InboxConflict))
    ));
}
