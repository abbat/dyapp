//! Public photo reads over trusted stored history; this is not upload or MLS receive coverage.
use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use super::{ChatSession, Store};
use crate::{ChatItem, ChatPhoto, CoreError};
use dyapp_client::attachment::{self, Attachment};
use dyapp_client::chat::{AttachmentMessage, Body};
use dyapp_client::store::messages::VerifiedChatSender;
use dyapp_client::store::originals;
use image::{ImageFormat, RgbImage};
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn jpeg() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::new(3, 2)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    bytes.into_inner()
}

fn record(
    f: &Fixture,
    peer: &str,
    bytes: &[u8],
    width: u32,
    height: u32,
) -> (ChatPhoto, Attachment) {
    let (mut attachment, _) = attachment::seal(bytes).unwrap();
    attachment.mime = "image/jpeg".into();
    attachment.width = width;
    attachment.height = height;
    attachment.preview = jpeg();
    let group = f
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .known_groups()
        .unwrap()
        .into_iter()
        .find(|(_, partner)| partner == peer)
        .unwrap()
        .0;
    let mut store = Store::open(&Path::new(&f.dir).join("client.db"), &f.key.identity).unwrap();
    // An older authenticated item arriving later must still sort before the existing text.
    let hlc = store
        .current_chat_messages(peer)
        .unwrap()
        .into_iter()
        .filter_map(|message| message.original_hlc)
        .map(u64::from)
        .min()
        .unwrap()
        .checked_sub(1)
        .unwrap();
    let body = Body::Attachment(AttachmentMessage {
        id: [42; 16],
        hlc,
        attachment: attachment.clone(),
    });
    // The Store API takes already-verified sender metadata. No network authentication is simulated.
    assert!(store
        .record_chat_attachment(
            VerifiedChatSender {
                group: &group,
                author: &f.key.peer_id(),
                device: &f.device.identity.public_key(),
            },
            &body,
            false,
        )
        .unwrap());
    let items = f.chats.chat_photos(peer.into()).unwrap();
    assert_eq!(items.len(), 1);
    (items.into_iter().next().unwrap(), attachment)
}

fn read(chats: &ChatSession, peer: &str, item: &ChatPhoto) -> Result<Vec<u8>, CoreError> {
    chats.chat_photo_original(
        peer.into(),
        item.group_id.clone(),
        item.author.clone(),
        item.id.clone(),
    )
}

fn keep(f: &Fixture, attachment: &Attachment, bytes: &[u8]) -> PathBuf {
    originals::keep(Path::new(&f.dir), &f.chats.seal_key, attachment, bytes).unwrap();
    let mut files = std::fs::read_dir(Path::new(&f.dir).join("originals"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    files.pop().unwrap()
}

fn ids(items: &[ChatItem]) -> Vec<Vec<u8>> {
    items
        .iter()
        .map(|item| match item {
            ChatItem::Text { message } => message.id.clone(),
            ChatItem::Photo { photo } => photo.id.clone(),
            ChatItem::Sticker { .. } => panic!("this fixture has only text and photos"),
        })
        .collect()
}

#[test]
fn download_policy_survives_reopening_every_mode() {
    use crate::ChatPhotoDownload;

    let mut fixture = Fixture::new();
    for mode in [
        ChatPhotoDownload::Always,
        ChatPhotoDownload::OnTap,
        ChatPhotoDownload::ByNetwork,
    ] {
        fixture.chats.set_photo_download(mode).unwrap();
        fixture = fixture.reopen();
        assert_eq!(fixture.chats.photo_download().unwrap(), mode);
    }
}

#[test]
fn populated_photo_history_and_cached_original_survive_native_reopen_offline() {
    let f = Fixture::new();
    let peer = establish(&f);
    let first = ids(&f.chats.chat_items(peer.clone()).unwrap());
    assert_eq!(first.len(), 1);
    let bytes = jpeg();
    let (item, attachment) = record(&f, &peer, &bytes, 3, 2);
    let group = item.group_id.clone();
    let last = f
        .chats
        .session
        .lock()
        .unwrap()
        .stage_text(&group, b"after photo", &f.chats.seal_key)
        .unwrap()
        .0
        .to_vec();
    let expected = vec![item.id.clone(), first[0].clone(), last];
    let thread = f.chats.chat_thread(peer.clone()).unwrap();
    let text = f.chats.chat_history(peer.clone()).unwrap();
    assert_eq!(thread.state, text.state);
    assert_eq!(thread.muted, text.muted);
    assert_eq!(ids(&thread.items), expected);
    let projected_text: Vec<_> = thread
        .items
        .into_iter()
        .filter_map(|row| match row {
            ChatItem::Text { message } => Some(message),
            ChatItem::Photo { .. } | ChatItem::Sticker { .. } => None,
        })
        .collect();
    assert_eq!(projected_text, text.messages);
    let file = keep(&f, &attachment, &bytes);
    let sealed = std::fs::read(&file).unwrap();
    assert!(sealed.len() > bytes.len());
    f.chats
        .set_photo_download(crate::ChatPhotoDownload::OnTap)
        .unwrap();
    let before = snapshot(&f);
    assert_eq!(
        f.chats
            .chat_photo_automatic(
                peer.clone(),
                item.group_id.clone(),
                item.author.clone(),
                item.id.clone(),
            )
            .unwrap_err()
            .to_string(),
        "onTap"
    );
    assert_eq!(read(&f.chats, &peer, &item).unwrap(), bytes);
    assert_eq!(snapshot(&f), before);

    let f = f.reopen();
    assert_eq!(ids(&f.chats.chat_items(peer.clone()).unwrap()), expected);
    assert_eq!(
        f.chats.chat_photos(peer.clone()).unwrap(),
        std::slice::from_ref(&item)
    );
    assert_eq!(read(&f.chats, &peer, &item).unwrap(), bytes);
    assert_eq!(snapshot(&f), before);
    assert_eq!(std::fs::read(file).unwrap(), sealed);
}

fn check_selectors(f: &Fixture, peer: &str, item: &ChatPhoto) {
    let mut changed = item.clone();
    changed.group_id.push(0);
    assert_eq!(
        read(&f.chats, peer, &changed).unwrap_err().to_string(),
        "notFound"
    );
    changed = item.clone();
    changed.author.push('0');
    assert_eq!(
        read(&f.chats, peer, &changed).unwrap_err().to_string(),
        "notFound"
    );
    changed = item.clone();
    changed.id[0] ^= 1;
    assert_eq!(
        read(&f.chats, peer, &changed).unwrap_err().to_string(),
        "notFound"
    );
    changed.id.clear();
    assert_eq!(
        read(&f.chats, peer, &changed).unwrap_err().to_string(),
        "notFound"
    );
    assert!(read(&f.chats, &f.key.peer_id(), item).is_err());
}

#[test]
fn cached_original_cannot_bypass_the_full_selector_block_or_clear() {
    let f = Fixture::new();
    let peer = establish(&f);
    let bytes = jpeg();
    let (item, attachment) = record(&f, &peer, &bytes, 3, 2);
    let file = keep(&f, &attachment, &bytes);
    let sealed = std::fs::read(&file).unwrap();
    let before = snapshot(&f);
    check_selectors(&f, &peer, &item);
    assert_eq!(snapshot(&f), before);
    assert_eq!(std::fs::read(&file).unwrap(), sealed);

    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    assert_eq!(
        read(&f.chats, &peer, &item).unwrap_err().to_string(),
        "blocked"
    );
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, false)
        .unwrap();
    assert_eq!(read(&f.chats, &peer, &item).unwrap(), bytes);
    f.chats.clear_chat(peer.clone()).unwrap();
    assert_eq!(
        read(&f.chats, &peer, &item).unwrap_err().to_string(),
        "notFound"
    );
}

#[test]
fn authenticated_invalid_bytes_or_descriptor_geometry_are_refused_from_cache() {
    for (bytes, width, height) in [(b"not an image".to_vec(), 3, 2), (jpeg(), 4, 2)] {
        let f = Fixture::new();
        let peer = establish(&f);
        let (item, attachment) = record(&f, &peer, &bytes, width, height);
        let file = keep(&f, &attachment, &bytes);
        let before = snapshot(&f);
        assert_eq!(
            read(&f.chats, &peer, &item).unwrap_err().to_string(),
            "corrupted"
        );
        assert_eq!(snapshot(&f), before);
        // A framing refusal does not silently replace the authenticated cache bytes.
        assert_eq!(
            originals::read(Path::new(&f.dir), &f.chats.seal_key, &attachment).unwrap(),
            bytes
        );
        assert!(file.is_file());
    }
}

#[test]
fn torn_cache_data_is_discarded_and_cannot_reach_a_native_decoder() {
    let f = Fixture::new();
    let peer = establish(&f);
    let bytes = jpeg();
    let (item, attachment) = record(&f, &peer, &bytes, 3, 2);
    let file = keep(&f, &attachment, &bytes);
    std::fs::write(&file, b"short").unwrap();
    let before = snapshot(&f);
    assert_eq!(
        read(&f.chats, &peer, &item).unwrap_err().to_string(),
        "notConnected"
    );
    assert!(!originals::is_kept(
        Path::new(&f.dir),
        &attachment.blob_sha256
    ));
    assert_eq!(snapshot(&f), before);
}

#[test]
fn incoming_jpeg_and_png_with_unspecified_geometry_reopen_from_cache_offline() {
    for format in [ImageFormat::Jpeg, ImageFormat::Png] {
        let f = Fixture::new();
        let peer = establish(&f);
        let mut encoded = Cursor::new(Vec::new());
        let bytes = if format == ImageFormat::Png {
            include_bytes!("../../../../client/src/attachment/original/validate/three-by-two.png")
                .to_vec()
        } else {
            RgbImage::new(3, 2).write_to(&mut encoded, format).unwrap();
            encoded.into_inner()
        };
        // MIME claims do not override the actual format; zero geometry is valid on the wire.
        let (item, attachment) = record(&f, &peer, &bytes, 0, 0);
        keep(&f, &attachment, &bytes);
        let f = f.reopen();
        let before = snapshot(&f);
        assert_eq!(read(&f.chats, &peer, &item).unwrap(), bytes);
        assert_eq!(snapshot(&f), before);
    }
}
