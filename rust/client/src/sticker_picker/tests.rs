use super::*;
use image_webp::{ColorType, WebPEncoder};
use prost::Message;

fn pack(count: usize, width: u32, height: u32) -> Pack {
    pack_labels(&vec!["😺"; count], width, height)
}

pub(super) fn pack_labels(labels: &[&str], width: u32, height: u32) -> Pack {
    #[derive(Clone, PartialEq, Message)]
    struct Record {
        #[prost(bytes = "vec", tag = "1")]
        manifest: Vec<u8>,
        #[prost(bytes = "vec", repeated, tag = "2")]
        files: Vec<Vec<u8>>,
    }
    let mut image = Vec::new();
    let pixels = [1, 2, 3, 255].repeat((width * height) as usize);
    WebPEncoder::new(&mut image)
        .encode(&pixels, width, height, ColorType::Rgba8)
        .unwrap();
    let entries: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(index, emoji)| {
            serde_json::json!({
                "file": format!("{index}.webp"), "emoji": emoji,
                "sha256": dyapp_identity::hex(&dyapp_identity::sha256(&image))
            })
        })
        .collect();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format": 1, "name": "Cats", "stickers": entries
    }))
    .unwrap();
    let id = dyapp_identity::sha256(&manifest);
    crate::sticker_pack::from_peer(
        &Record {
            manifest,
            files: vec![image; labels.len()],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

#[test]
fn pages_keep_manifest_order_and_literal_eight_entry_boundary() {
    let pack = pack(9, 1, 1);
    let first = page(&pack, 0).unwrap();
    assert_eq!(first.pack_id, *pack.id());
    assert_eq!(first.name, "Cats");
    assert_eq!(first.total, 9);
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(8));
    for (index, choice) in first.entries.iter().enumerate() {
        assert_eq!(choice.index, index);
        assert_eq!(choice.emoji, pack.stickers()[index].emoji);
    }
    let last = page(&pack, first.next.unwrap()).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].index, 8);
    assert_eq!(last.next, None);
    let empty = page(&pack, 9).unwrap();
    assert!(empty.entries.is_empty() && empty.next.is_none());
    assert_eq!(empty.total, 9);
}

#[test]
fn invalid_offsets_and_choices_are_errors_even_at_usize_max() {
    let pack = pack(1, 1, 1);
    for index in [1, 2, usize::MAX] {
        assert!(matches!(thumbnail(&pack, index), Err(Error::InvalidIndex)));
    }
    for after in [2, usize::MAX] {
        assert!(matches!(page(&pack, after), Err(Error::InvalidIndex)));
    }
}

#[test]
fn wide_and_tall_thumbnails_keep_bounded_rgba_and_literal_96px_limit() {
    for (width, height) in [(128, 64), (64, 128)] {
        let pack = pack(1, width, height);
        let choice = thumbnail(&pack, 0).unwrap();
        let frame = choice.frame;
        assert!((1..=96).contains(&frame.width));
        assert!((1..=96).contains(&frame.height));
        assert_eq!(
            frame.pixels,
            [1, 2, 3, 255].repeat((frame.width * frame.height) as usize)
        );
        assert_eq!(choice.emoji, "😺");
    }
}
