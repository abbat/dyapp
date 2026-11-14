//! Public chat-sticker reads use real staged MLS history and the shared media snapshot.
use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use super::saved_sticker::{pack, save};
use super::*;
use crate::{ChatSticker, ChatStickerFrame};

fn stage(f: &Fixture, peer: &str, attached: bool) -> ChatSticker {
    let pack = pack();
    let bytes = pack.sticker_bytes(0).unwrap();
    let frame = dyapp_client::sticker_webp::decode::first_frame(bytes).unwrap();
    let (mut descriptor, _) = dyapp_client::attachment::seal(bytes).unwrap();
    descriptor.mime = "image/webp".into();
    descriptor.width = frame.width;
    descriptor.height = frame.height;
    descriptor.release_secret = [0; 32];
    let template = dyapp_client::chat::sticker::Sticker {
        id: [0; 16],
        hlc: 0,
        pack_id: *pack.id(),
        sha256: pack.stickers()[0].sha256,
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: attached.then_some(descriptor),
    };
    let mut session = f.chats.session.lock().unwrap();
    let group = session
        .store()
        .known_groups()
        .unwrap()
        .into_iter()
        .find(|(_, partner)| partner == peer)
        .unwrap()
        .0;
    let (id, _) = session
        .stage_sticker(&group, &template, &f.chats.seal_key)
        .unwrap();
    session
        .store()
        .current_chat_stickers(peer)
        .unwrap()
        .into_iter()
        .find(|item| item.key.id == id)
        .unwrap()
        .into()
}

fn read(
    f: &Fixture,
    peer: &str,
    item: &ChatSticker,
    manual: bool,
) -> Result<ChatStickerFrame, CoreError> {
    f.chats.chat_sticker_frame(
        peer.into(),
        item.group_id.clone(),
        item.author.clone(),
        item.id.clone(),
        manual,
    )
}

#[test]
fn saved_message_fallback_returns_exact_core_pixels_without_network_or_new_mail() {
    let f = Fixture::new();
    let peer = establish(&f);
    let item = stage(&f, &peer, false);
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    f.chats
        .set_photo_download(super::super::ChatPhotoDownload::OnTap)
        .unwrap();
    let expected =
        dyapp_client::sticker_webp::decode::first_frame(pack.sticker_bytes(0).unwrap()).unwrap();
    let before = snapshot(&f);
    let frame = read(&f, &peer, &item, false).unwrap();
    assert_eq!(
        (frame.width, frame.height),
        (expected.width, expected.height)
    );
    assert_eq!(frame.pixels, expected.pixels);
    assert_eq!(snapshot(&f), before);
}

#[test]
fn full_key_block_and_cleared_history_refuse_before_exposing_local_saved_pixels() {
    let f = Fixture::new();
    let peer = establish(&f);
    let item = stage(&f, &peer, false);
    save(&f, &pack(), &f.chats.seal_key);
    for changed in [
        ChatSticker {
            group_id: vec![0],
            ..item.clone()
        },
        ChatSticker {
            author: "other".into(),
            ..item.clone()
        },
        ChatSticker {
            id: vec![0; 16],
            ..item.clone()
        },
        ChatSticker {
            id: vec![],
            ..item.clone()
        },
    ] {
        assert_eq!(
            read(&f, &peer, &changed, true).unwrap_err().to_string(),
            "unknownMessage"
        );
    }
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    assert_eq!(
        read(&f, &peer, &item, true).unwrap_err().to_string(),
        "blocked"
    );
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, false)
        .unwrap();
    f.chats.clear_chat(peer.clone()).unwrap();
    assert_eq!(
        read(&f, &peer, &item, true).unwrap_err().to_string(),
        "unknownMessage"
    );
}

#[test]
fn automatic_refusal_manual_network_failure_and_missing_descriptor_stay_distinct() {
    let f = Fixture::new();
    let peer = establish(&f);
    let item = stage(&f, &peer, true);
    f.chats
        .set_photo_download(super::super::ChatPhotoDownload::OnTap)
        .unwrap();
    let before = snapshot(&f);
    assert_eq!(
        read(&f, &peer, &item, false).unwrap_err().to_string(),
        "onTap"
    );
    assert_eq!(
        read(&f, &peer, &item, true).unwrap_err().to_string(),
        "notConnected"
    );
    assert_eq!(
        read(&f, &f.key.peer_id(), &item, true)
            .unwrap_err()
            .to_string(),
        "ownPeer"
    );
    assert_eq!(snapshot(&f), before);
    let missing = stage(&f, &peer, false);
    assert_eq!(
        read(&f, &peer, &missing, true).unwrap_err().to_string(),
        "notFound"
    );
}
