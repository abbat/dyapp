//! The exported native read must revalidate durable current state after real network completion.
use super::favorites::Fixture;
use super::offline::{establish, snapshot, snapshot_store};
use super::saved_sticker::pack;
use super::sticker_original::stage_download;
use super::*;
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;

#[path = "sticker_download/controlled.rs"]
mod controlled;

fn phase<T>(runtime: &Runtime, label: &str, work: impl Future<Output = T>) -> T {
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), work)
            .await
            .expect(label)
    })
}

fn change(f: &Fixture, peer: &str, action: &str) -> (Option<Vec<u8>>, Vec<Vec<u8>>) {
    let mut store = Store::open(
        &std::path::Path::new(&f.dir).join("client.db"),
        &f.key.identity,
    )
    .unwrap();
    match action {
        "block" => store.set_blocked(peer, true).unwrap(),
        "clear" => {
            store.clear_chat_pair(peer).unwrap();
        }
        _ => {}
    }
    // The gated native reader holds its session mutex; inspect durable state independently.
    snapshot_store(&store)
}

fn check_result(f: &Fixture, action: &str, result: Result<crate::ChatStickerFrame, CoreError>) {
    if action == "unchanged" {
        let frame = result.unwrap();
        let pack = pack();
        let expected =
            dyapp_client::sticker_webp::decode::first_frame(pack.sticker_bytes(0).unwrap())
                .unwrap();
        assert_eq!(
            (frame.width, frame.height, frame.pixels),
            (expected.width, expected.height, expected.pixels)
        );
        assert!(f.chats.media_cache().unwrap().used_bytes > 0);
    } else {
        assert_eq!(
            result.unwrap_err().to_string(),
            if action == "block" {
                "blocked"
            } else {
                "unknownMessage"
            }
        );
        assert_eq!(f.chats.media_cache().unwrap().used_bytes, 0);
    }
}

fn completion_case(runtime: &Runtime, action: &str) {
    let f = Fixture::new();
    let peer = establish(&f);
    let (item, blob) = stage_download(&f, &peer, true);
    f.chats.set_media_cache(true, 1_048_576).unwrap();
    let mut server = controlled::Controlled::start(runtime, blob);
    dyapp_identity::network::save(
        std::path::Path::new(&f.dir),
        dyapp_identity::network::Settings::Active,
        &dyapp_identity::network::Network {
            seeds: vec![server.seed.clone()],
            name: String::new(),
        },
    )
    .unwrap();
    let chats = Arc::clone(&f.chats);
    let target = peer.clone();
    let result = runtime.spawn_blocking(move || {
        chats.chat_sticker_frame(target, item.group_id, item.author, item.id, true)
    });
    phase(runtime, "native MediaGet arrival", &mut server.requested).unwrap();
    // Capture after the deliberate mutation, without waiting on the gated reader's mutex.
    let before = change(&f, &peer, action);
    server.release.take().unwrap().send(()).unwrap();
    let result = phase(runtime, "native media completion", result).unwrap();
    check_result(&f, action, result);
    assert_eq!(snapshot(&f), before);
}

#[test]
fn actual_native_media_completion_rechecks_block_clear_and_success_before_cache_retention() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    for action in ["unchanged", "block", "clear"] {
        completion_case(&runtime, action);
    }
}
