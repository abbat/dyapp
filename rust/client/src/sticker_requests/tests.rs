use super::*;
use crate::chat::pack_control::PackAnswer;
use crate::sticker_media::tests::Fixture;
use crate::store::messages::VerifiedChatSender;
use crate::store::InboxKey;

const SEAL: [u8; 32] = [5; 32];

fn read(f: &mut Fixture) -> Status {
    status(&mut f.store, &SEAL, &f.peer, &f.key).unwrap()
}

fn request(f: &mut Fixture) -> PendingPack {
    let request = f.store.sticker_pack_request(&f.key.group, &f.key).unwrap();
    f.store
        .stage_sticker_pack_request(b"sealed fixture", &[], &f.key.group, &f.key, &request)
        .unwrap()
}

fn answer(f: &mut Fixture) {
    let (mut attachment, _) = crate::attachment::seal(&f.pack.encode()).unwrap();
    attachment.release_secret = [0; 32];
    assert!(f
        .store
        .stage_sticker_pack_answer(
            b"sealed fixture",
            &InboxKey {
                mailbox: [1; 32],
                id: [77; 16]
            },
            VerifiedChatSender {
                group: &f.key.group,
                author: &f.peer,
                device: &[8; 32]
            },
            &PackAnswer {
                pack_id: *f.pack.id(),
                attachment
            },
        )
        .unwrap());
}

#[test]
fn own_authored_sticker_is_not_a_received_pack_save_source() {
    let mut f = Fixture::new(true, false);
    let own = f.store.owner_peer_id().to_owned();
    let mut item = f
        .store
        .current_chat_stickers(&f.peer)
        .unwrap()
        .remove(0)
        .sticker;
    item.id = [3; 16];
    f.store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &f.key.group,
                author: &own,
                device: &[8; 32],
            },
            &item,
            false,
        )
        .unwrap();
    f.key.author = own;
    f.key.id = item.id;
    assert!(matches!(
        status(&mut f.store, &SEAL, &f.peer, &f.key),
        Err(Error::Media(sticker_media::Error::UnknownMessage))
    ));
}

#[test]
fn new_and_saved_states_follow_authenticated_local_content() {
    let mut f = Fixture::new(true, false);
    assert_eq!(read(&mut f), Status::plain(State::New, None));
    f.store.save_sticker_pack(&SEAL, &f.pack).unwrap();
    assert_eq!(read(&mut f), Status::plain(State::Saved, None));
    assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
}

#[test]
fn pending_answer_and_failure_states_retain_exact_generation_and_core_deadline() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    assert_eq!(
        read(&mut f),
        Status::plain(State::Waiting, Some(pending.token))
    );
    answer(&mut f);
    let downloading = read(&mut f);
    assert_eq!(downloading.state, State::Downloading);
    assert_eq!(downloading.generation, Some(pending.token));
    assert_eq!(downloading.retry_at_ms, Some(0));
    let job = PackDownloadJob {
        peer: f.peer.clone(),
        pack_id: *f.pack.id(),
        sticker_hash: pending.sticker_hash,
        token: pending.token,
    };
    for failure in [
        PackDownloadFailure::Fetch,
        PackDownloadFailure::Invalid,
        PackDownloadFailure::Quota,
        PackDownloadFailure::Storage,
    ] {
        f.store
            .defer_sticker_pack_download(&job, failure, 1_000)
            .unwrap();
        let stored = f
            .store
            .sticker_pack_download_state(&f.peer, f.pack.id())
            .unwrap()
            .unwrap();
        let current = read(&mut f);
        assert_eq!(current.state, State::Failed);
        assert_eq!(current.failure, Some(Failure::from(failure)));
        assert_eq!(current.generation, Some(pending.token));
        assert_eq!(current.retry_at_ms, Some(stored.retry_at_ms));
    }
}

#[test]
fn valid_saved_status_reconciles_pending_intent_without_changing_saved_bytes() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    f.store.save_sticker_pack(&SEAL, &f.pack).unwrap();
    let file = f
        .dir
        .join("sticker-packs")
        .join(dyapp_identity::hex(f.pack.id()));
    let before = std::fs::read(&file).unwrap();
    assert_eq!(
        f.store
            .pending_sticker_pack(&f.peer, f.pack.id())
            .unwrap()
            .unwrap()
            .token,
        pending.token
    );
    assert_eq!(read(&mut f), Status::plain(State::Saved, None));
    assert!(f
        .store
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
    assert_eq!(std::fs::read(file).unwrap(), before);
}

#[test]
fn unreadable_pack_is_explicit_storage_failure_with_generation_and_no_deletion() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    f.store.save_sticker_pack(&SEAL, &f.pack).unwrap();
    let file = f
        .dir
        .join("sticker-packs")
        .join(dyapp_identity::hex(f.pack.id()));
    std::fs::write(&file, b"unreadable saved fixture").unwrap();
    assert_eq!(
        read(&mut f),
        Status::failed(Failure::Storage, Some(pending.token))
    );
    assert_eq!(std::fs::read(file).unwrap(), b"unreadable saved fixture");
    assert!(f
        .store
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_some());
}

#[test]
fn saved_pack_without_clicked_hash_never_reconciles_unrelated_intent() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    f.store.save_sticker_pack(&SEAL, &f.pack).unwrap();
    let mut item = f
        .store
        .current_chat_stickers(&f.peer)
        .unwrap()
        .remove(0)
        .sticker;
    item.id = [2; 16];
    item.sha256 = [99; 32];
    f.store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &f.key.group,
                author: &f.peer,
                device: &[8; 32],
            },
            &item,
            true,
        )
        .unwrap();
    f.key.id = item.id;
    assert_eq!(
        read(&mut f),
        Status::failed(Failure::StickerMissing, Some(pending.token))
    );
    assert!(f
        .store
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_some());
}
