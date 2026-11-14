use super::*;

fn fixture() -> (ChatSticker, Selection, Vec<u8>) {
    let bytes = include_bytes!("../../../../tests/fuzz/corpus/sticker_webp/lossy.webp").to_vec();
    let frame = dyapp_client::sticker_webp::decode::first_frame(&bytes).unwrap();
    let (mut descriptor, _) = attachment::seal(&bytes).unwrap();
    descriptor.release_secret = [0; 32];
    descriptor.mime = "image/webp".into();
    descriptor.width = frame.width;
    descriptor.height = frame.height;
    let selected = Selection {
        peer_id: "peer".into(),
        group_id: vec![9],
        author: "peer".into(),
        id: [1; 16],
    };
    let item = ChatSticker {
        key: selected.key(),
        unread: true,
        delivery: None,
        sticker: dyapp_client::chat::sticker::Sticker {
            id: selected.id,
            hlc: 1,
            pack_id: [2; 32],
            sha256: dyapp_identity::sha256(&bytes),
            emoji: "😺".into(),
            pack_name: "Cats".into(),
            attachment: Some(descriptor),
        },
    };
    (item, selected, bytes)
}

#[test]
fn message_selection_projects_the_full_native_key() {
    let (item, selection, _) = fixture();
    let key = selection.key();
    assert_eq!(key.group, selection.group_id);
    assert_eq!(key.author, selection.author);
    assert_eq!(key.id, selection.id);
    assert!(key == item.key);
}

#[test]
fn native_frame_is_only_dimensions_and_hash_bound_rgba() {
    let (item, _, bytes) = fixture();
    let frame = item.sticker.first_frame(&bytes).unwrap();
    let width = frame.width;
    let height = frame.height;
    let pixels = frame.pixels.clone();
    let binary = frame_bytes(frame);
    assert_eq!(&binary[..4], width.to_le_bytes());
    assert_eq!(&binary[4..8], height.to_le_bytes());
    assert_eq!(binary.len(), 8 + width as usize * height as usize * 4);
    assert_eq!(binary[8..], pixels);
    let mut wrong = item.sticker;
    wrong.sha256 = [99; 32];
    assert!(wrong.first_frame(&bytes).is_err());
    assert!(wrong.first_frame(b"not WebP").is_err());
}
