//! Old protected media must not become history on a newly enrolled native receiver.
use super::super::super::media::{match_media_chat, media_node_at};
use super::{native_at, ExistingChat, SEAL};
use dyapp_client::attachment::{self, Attachment};
use dyapp_client::chat::sticker::Sticker;
use dyapp_client::store::messages::ChatItem;
use std::slice::from_mut;

// Independent libwebp 1.5.0 vector used by the common decoder's reference test.
const WEBP: &[u8] = &[
    82, 73, 70, 70, 46, 0, 0, 0, 87, 69, 66, 80, 86, 80, 56, 32, 34, 0, 0, 0, 112, 1, 0, 157, 1,
    42, 1, 0, 1, 0, 1, 64, 38, 37, 148, 2, 116, 1, 64, 0, 0, 254, 252, 55, 129, 87, 247, 212, 232,
    62, 43, 224, 0, 0,
];

async fn photo(chat: &mut ExistingChat) -> (Attachment, Vec<u8>) {
    let jpeg =
        dyapp_client::photo::make_preview(&[255, 0, 0, 255], 1, 1, dyapp_client::photo::MAX_BYTES)
            .unwrap();
    let (mut descriptor, sealed) = attachment::seal(&jpeg).unwrap();
    descriptor.preview = jpeg.clone();
    descriptor.mime = "image/jpeg".into();
    descriptor.width = 1;
    descriptor.height = 1;
    chat.sender
        .send_attachment_to(
            from_mut(&mut chat.alice.node),
            &chat.alice.identity,
            &chat.bob.identity.peer_id(),
            &descriptor,
            &sealed,
            &SEAL,
        )
        .await
        .unwrap();
    protected_blob(chat, &descriptor, &jpeg).await;
    (descriptor, jpeg)
}

async fn protected_blob(chat: &mut ExistingChat, descriptor: &Attachment, plaintext: &[u8]) {
    let bytes = chat
        .bob
        .node
        .get_media(&descriptor.blob_sha256)
        .await
        .unwrap();
    assert_ne!(bytes, plaintext);
    assert_eq!(attachment::open(descriptor, &bytes).unwrap(), plaintext);
}

async fn protected_originals(chat: &mut ExistingChat, originals: &[(Attachment, Vec<u8>)]) {
    for (descriptor, bytes) in originals {
        protected_blob(chat, descriptor, bytes).await;
    }
}

async fn sticker(chat: &mut ExistingChat) -> Attachment {
    let frame = dyapp_client::sticker_webp::decode::first_frame(WEBP).unwrap();
    assert_eq!((frame.width, frame.height), (1, 1));
    let (mut descriptor, sealed) = attachment::seal(WEBP).unwrap();
    descriptor.mime = "image/webp".into();
    descriptor.width = frame.width;
    descriptor.height = frame.height;
    chat.alice
        .node
        .attach_media(&chat.alice.identity, &descriptor, dyapp_client::now())
        .await
        .unwrap();
    chat.alice
        .node
        .put_media(&chat.alice.identity.peer_id(), sealed)
        .await
        .unwrap();
    protected_blob(chat, &descriptor, WEBP).await;
    descriptor.release_secret = [0; 32];
    let sticker = Sticker {
        id: [0; 16],
        hlc: 0,
        pack_id: [5; 32],
        sha256: descriptor.plaintext_sha256,
        emoji: "\u{1f63a}".into(),
        pack_name: "Before enrollment".into(),
        attachment: Some(descriptor),
    };
    chat.sender
        .send_sticker(from_mut(&mut chat.alice.node), &chat.group, &sticker, &SEAL)
        .await
        .unwrap();
    sticker.attachment.unwrap()
}

async fn seed_old_media(chat: &mut ExistingChat) -> [(Attachment, Vec<u8>); 2] {
    match_media_chat(
        &mut chat.alice,
        &mut chat.bob,
        &mut chat.sender,
        &mut chat.contact,
        &chat.held,
        &chat.group,
    )
    .await;
    let photo = photo(chat).await;
    let sticker = sticker(chat).await;
    chat.contact
        .receive(
            from_mut(&mut chat.bob.node),
            &chat.bob.device_key,
            &chat.held,
            &SEAL,
        )
        .await
        .unwrap();
    let items = chat
        .contact
        .store()
        .current_chat_items(&chat.alice.identity.peer_id())
        .unwrap();
    assert!(items
        .iter()
        .any(|item| matches!(item, ChatItem::Attachment(_))));
    assert!(items
        .iter()
        .any(|item| matches!(item, ChatItem::Sticker(_))));
    [photo, (sticker, WEBP.to_vec())]
}

#[test]
fn native_linked_receiver_excludes_old_photo_and_sticker_but_receives_new_traffic() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let address = runtime.block_on(media_node_at(1000));
    let mut chat = runtime.block_on(ExistingChat::at(address));
    let originals = runtime.block_on(seed_old_media(&mut chat));
    let laptop = runtime.block_on(chat.link());
    runtime.block_on(protected_originals(&mut chat, &originals));
    let dir = test_dir.to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let peer = chat.bob.identity.peer_id();
    let receiver = native_at(&laptop, &chat.address, &dir);
    assert!(receiver.receive().unwrap() > 0);
    assert_eq!(receiver.chat_items(peer.clone()).unwrap().len(), 0);
    assert_eq!(receiver.media_cache().unwrap().used_bytes, 0);
    runtime.block_on(chat.deliver_new_message());
    receiver.receive().unwrap();
    let items = receiver.chat_items(peer.clone()).unwrap();
    assert!(
        matches!(items.as_slice(), [dyapp_ffi::ChatItem::Text { message }] if message.text.as_deref() == Some("after enrollment"))
    );
    assert_eq!(receiver.media_cache().unwrap().used_bytes, 0);
    drop(receiver);
    let reopened = native_at(&laptop, &chat.address, &dir);
    reopened.receive().unwrap();
    let items = reopened.chat_items(peer).unwrap();
    assert!(
        matches!(items.as_slice(), [dyapp_ffi::ChatItem::Text { message }] if message.text.as_deref() == Some("after enrollment"))
    );
    assert_eq!(reopened.media_cache().unwrap().used_bytes, 0);
    runtime.block_on(protected_originals(&mut chat, &originals));
}
