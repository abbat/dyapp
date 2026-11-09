use super::*;

#[test]
fn picker_pages_are_bounded_and_contain_only_labels_and_resized_pixels() {
    let pack =
        dyapp_client::sticker_pack::import_tar(&super::super::native_tests::archive_count(9))
            .unwrap();
    let first = page(&pack, 0).unwrap();
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(8));
    assert_eq!(first.total, 9);
    let second = page(&pack, 8).unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.entries[0].index, 8);
    assert_eq!(second.next, None);
    assert!(page(&pack, 9).unwrap().entries.is_empty());
    assert!(page(&pack, 10).is_err());
    assert!(thumbnail(&pack, 9).is_err());
    let json = serde_json::to_value(first).unwrap();
    for (index, entry) in json["entries"].as_array().unwrap().iter().enumerate() {
        assert_serialized_entry(index, entry);
    }
}

fn assert_serialized_entry(index: usize, entry: &serde_json::Value) {
    assert_eq!(entry["index"], index);
    let mut fields: Vec<_> = entry
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["emoji", "frame", "index"]);
    assert_frame(&entry["frame"]);
}

fn assert_frame(frame: &serde_json::Value) {
    let mut fields: Vec<_> = frame
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["height", "pixels", "width"]);
    let width = frame["width"].as_u64().unwrap();
    let height = frame["height"].as_u64().unwrap();
    assert!((1..=96).contains(&width) && (1..=96).contains(&height));
    assert_eq!(
        frame["pixels"].as_array().unwrap().len() as u64,
        width * height * 4
    );
}
