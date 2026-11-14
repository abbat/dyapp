use super::*;

#[test]
fn recent_projection_preserves_flat_fields_exact_selection_and_cursor() {
    let bytes = super::super::super::native_tests::archive_labels(&["😺", "🐱"]);
    let pack = dyapp_client::sticker_pack::import_tar(&bytes).unwrap();
    let projected = PageView::from(recents::Page {
        entries: vec![recents::Entry {
            pack_id: *pack.id(),
            pack_name: pack.name().into(),
            sticker: dyapp_client::sticker_picker::thumbnail(&pack, 1).unwrap(),
        }],
        next: Some(7),
    });
    let json = serde_json::to_value(projected).unwrap();
    assert_eq!(json["next"], 7);
    let entry = &json["entries"][0];
    let mut keys: Vec<_> = entry
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["emoji", "frame", "index", "packId", "packName"]);
    assert_eq!(entry["packId"], serde_json::json!(pack.id()));
    assert_eq!(entry["packName"], "Animals");
    assert_eq!(entry["index"], 1);
    assert_eq!(entry["emoji"], "🐱");
    let mut frame_keys: Vec<_> = entry["frame"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    frame_keys.sort_unstable();
    assert_eq!(frame_keys, ["height", "pixels", "width"]);
}

#[test]
fn empty_recent_projection_keeps_empty_entries_and_absent_next() {
    let value = PageView::from(recents::Page {
        entries: Vec::new(),
        next: None,
    });
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({ "entries": [], "next": null })
    );
}
