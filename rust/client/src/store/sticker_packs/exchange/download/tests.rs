use super::*;
mod clear;
mod retry;
use crate::chat::pack_control::PackAnswer;
use crate::store::{
    messages::VerifiedChatSender,
    sticker_packs::exchange::{
        tests::{database, selected},
        PendingPack,
    },
    InboxKey,
};

fn request(store: &mut Store, peer: &str, id: u8) -> (PendingPack, PackAnswer) {
    let selected = selected(store, peer, id);
    let request = store.sticker_pack_request(&[9], &selected).unwrap();
    let pending = store
        .stage_sticker_pack_request(b"request", &[], &[9], &selected, &request)
        .unwrap();
    let (mut attachment, _) = crate::attachment::seal(b"pack fixture").unwrap();
    attachment.release_secret = [0; 32];
    (
        pending,
        PackAnswer {
            pack_id: [id; 32],
            attachment,
        },
    )
}
fn admit(store: &mut Store, peer: &str, id: u8, answer: &PackAnswer) -> bool {
    store
        .stage_sticker_pack_answer(
            b"answer",
            &InboxKey {
                mailbox: [9; 32],
                id: [id; 16],
            },
            VerifiedChatSender {
                group: &[9],
                author: peer,
                device: &[8; 32],
            },
            answer,
        )
        .unwrap()
}

#[test]
fn waiting_answer_failure_and_generation_safe_retry_survive_restart() {
    let (mut store, owner, path, peer) = database();
    let (pending, answer) = request(&mut store, &peer, 1);
    let waiting = store
        .sticker_pack_download_state(&peer, &answer.pack_id)
        .unwrap()
        .unwrap();
    assert!(!waiting.has_answer && waiting.failure.is_none());
    assert!(store.due_sticker_pack_downloads(0).unwrap().is_empty());
    assert!(admit(&mut store, &peer, 1, &answer));
    let job = store.due_sticker_pack_downloads(0).unwrap().remove(0);
    assert!(job.token == pending.token && job.sticker_hash == [7; 32]);
    assert!(store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Fetch, 100)
        .unwrap());
    let failed = store
        .sticker_pack_download_state(&peer, &answer.pack_id)
        .unwrap()
        .unwrap();
    assert_eq!(failed.failure, Some(PackDownloadFailure::Fetch));
    assert_eq!((failed.attempts, failed.retry_at_ms), (1, 1100));
    assert!(!admit(&mut store, &peer, 2, &answer));
    assert!(
        store
            .sticker_pack_download_state(&peer, &answer.pack_id)
            .unwrap()
            == Some(failed)
    );
    assert!(store.due_sticker_pack_downloads(1099).unwrap().is_empty());
    assert!(store.due_sticker_pack_downloads(1100).unwrap() == vec![job.clone()]);
    let sealed = store.sealed_device().unwrap();
    let next = store.retry_sticker_pack_download(&job).unwrap().unwrap();
    assert!(next.token != job.token && next.sticker_hash == job.sticker_hash);
    assert!(
        store
            .sticker_pack_answer(&[9], &peer, &answer.pack_id, &next.token)
            .unwrap()
            == Some(answer.clone())
    );
    assert!(store
        .sticker_pack_answer(&[9], &peer, &answer.pack_id, &job.token)
        .unwrap()
        .is_none());
    assert!(!store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Storage, 200)
        .unwrap());
    assert!(!store
        .finish_sticker_pack(&peer, &answer.pack_id, &job.token)
        .unwrap());
    assert!(store.retry_sticker_pack_download(&job).unwrap().is_none());
    assert!(store.sealed_device().unwrap() == sealed);
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.due_sticker_pack_downloads(0).unwrap() == vec![next]);
    let state = store
        .sticker_pack_download_state(&peer, &answer.pack_id)
        .unwrap()
        .unwrap();
    assert!(state.has_answer && state.failure.is_none() && state.attempts == 0);
}

#[test]
fn download_failure_codes_and_retry_cap_are_bounded_without_sleep() {
    let (mut store, _, _, peer) = database();
    let (_, answer) = request(&mut store, &peer, 1);
    admit(&mut store, &peer, 1, &answer);
    let job = store.due_sticker_pack_downloads(0).unwrap().remove(0);
    let mut now = 100;
    for (index, delay) in [
        1000, 2000, 4000, 8000, 16000, 32000, 64000, 128000, 256000, 512000, 1024000, 2048000,
        3600000, 3600000,
    ]
    .into_iter()
    .enumerate()
    {
        let code = [
            PackDownloadFailure::Fetch,
            PackDownloadFailure::Invalid,
            PackDownloadFailure::Quota,
            PackDownloadFailure::Storage,
        ][index % 4];
        assert!(store.defer_sticker_pack_download(&job, code, now).unwrap());
        let state = store
            .sticker_pack_download_state(&peer, &answer.pack_id)
            .unwrap()
            .unwrap();
        assert_eq!(state.failure, Some(code));
        assert_eq!(state.retry_at_ms, now + delay);
        assert!(store
            .due_sticker_pack_downloads(now + delay - 1)
            .unwrap()
            .is_empty());
        now += delay;
    }
    assert_eq!(
        store
            .sticker_pack_download_state(&peer, &answer.pack_id)
            .unwrap()
            .unwrap()
            .attempts,
        13
    );
    assert!(store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Fetch, u64::MAX)
        .is_err());
    assert!(store.due_sticker_pack_downloads(u64::MAX).is_err());
    assert!(failure(Some(99)).is_err());
}

#[test]
fn due_download_list_never_mounts_more_than_32_admitted_jobs() {
    let (mut store, _, _, peer) = database();
    for id in 1..=64 {
        let (_, answer) = request(&mut store, &peer, id);
        admit(&mut store, &peer, id, &answer);
    }
    assert_eq!(store.due_sticker_pack_downloads(0).unwrap().len(), 32);
}
