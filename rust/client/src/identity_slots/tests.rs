mod create;
mod open;
mod registry;

use super::*;
use std::fs;
use std::path::Path;

const FIRST: &str = "11111111-1111-1111-1111-111111111111";
const SECOND: &str = "22222222-2222-2222-2222-222222222222";

fn root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/slot-registry-tests")
        .join(format!("{:016x}", getrandom::u64().expect("fixture nonce")));
    fs::create_dir_all(&root).unwrap();
    root
}

fn slot(root: &Path, id: &str, state: &[u8]) -> PathBuf {
    let dir = root.join("profiles").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("state"), state).unwrap();
    dir
}

#[test]
fn existing_desktop_system_and_file_layouts_remain_readable_without_keys_moving() {
    let root = root();
    fs::write(root.join("identity-storage"), b"system\n").unwrap();
    let second = slot(&root, SECOND, b"ready\n");
    fs::write(second.join("storage-mode"), b"file\n").unwrap();
    fs::write(second.join("identity.key"), b"existing key bytes").unwrap();
    let first = slot(&root, FIRST, b"pending\n");
    fs::write(first.join("storage-mode"), b"system\n").unwrap();
    let slots = list(&root, Layout::Desktop).unwrap();
    assert_eq!(
        slots
            .iter()
            .map(|slot| slot.id.as_str())
            .collect::<Vec<_>>(),
        [LEGACY, FIRST, SECOND]
    );
    assert_eq!(
        slots.iter().map(|slot| slot.state).collect::<Vec<_>>(),
        [State::Ready, State::Pending, State::Ready]
    );
    assert_eq!(slots[0].storage, Some(Storage::System));
    assert_eq!(slots[2].storage, Some(Storage::File));
    assert_eq!(
        fs::read(second.join("identity.key")).unwrap(),
        b"existing key bytes"
    );
    assert!(!root.join("identity.key").exists());
    assert_eq!(active(&root, Layout::Desktop), Active::Absent);
}

#[test]
fn wrapped_layouts_need_no_desktop_mode_marker_and_keep_legacy_bytes() {
    for layout in [Layout::Android, Layout::Ios] {
        let root = root();
        fs::write(root.join("identity.key"), b"existing wrapped bytes").unwrap();
        slot(&root, FIRST, b"ready\n");
        let slots = list(&root, layout).unwrap();
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .all(|slot| slot.state == State::Ready && slot.storage == Some(Storage::Wrapped)));
        assert_eq!(
            fs::read(root.join("identity.key")).unwrap(),
            b"existing wrapped bytes"
        );
        assert_eq!(
            list(&root, Layout::Desktop).unwrap()[1].state,
            State::Broken
        );
    }
}

#[test]
fn malformed_entries_never_become_ready_and_foreign_names_are_ignored() {
    let root = root();
    slot(&root, FIRST, b"ready");
    fs::write(root.join("profiles").join(SECOND), b"not a directory").unwrap();
    slot(&root, "unrelated", b"ready\n");
    let slots = list(&root, Layout::Android).unwrap();
    assert_eq!(slots.len(), 2);
    assert!(slots.iter().all(|slot| slot.state == State::Broken));
    fs::write(root.join("active-profile"), [b'x'; 65]).unwrap();
    assert_eq!(active(&root, Layout::Android), Active::Invalid);
}

#[test]
fn apple_deletion_markers_override_slots_and_survive_directory_removal() {
    let root = root();
    slot(&root, FIRST, b"ready\n");
    for id in [FIRST, SECOND] {
        fs::write(
            root.join("profiles").join(format!("{id}.deleting")),
            b"deleting\n",
        )
        .unwrap();
    }
    let slots = list(&root, Layout::Ios).unwrap();
    assert_eq!(slots.len(), 2);
    assert!(slots.iter().all(|slot| slot.state == State::Deleting));
    assert_eq!(list(&root, Layout::Android).unwrap().len(), 1);
}

#[test]
fn pointer_formats_preserve_selection_clearing_and_android_missing_newline() {
    let root = root();
    for layout in [Layout::Desktop, Layout::Android, Layout::Ios] {
        fs::write(root.join("active-profile"), format!("{FIRST}\n")).unwrap();
        assert_eq!(active(&root, layout), Active::Selected(FIRST.into()));
        fs::write(root.join("active-profile"), b"none\n").unwrap();
        assert_eq!(
            active(&root, layout),
            if layout == Layout::Ios {
                Active::Unselected
            } else {
                Active::Invalid
            }
        );
    }
    fs::write(root.join("active-profile"), FIRST).unwrap();
    assert_eq!(
        active(&root, Layout::Android),
        Active::Selected(FIRST.into())
    );
    assert_eq!(active(&root, Layout::Desktop), Active::Invalid);
    assert_eq!(active(&root, Layout::Ios), Active::Invalid);
}

#[cfg(unix)]
#[test]
fn symlinked_markers_slots_and_inventory_roots_are_not_followed() {
    use std::os::unix::fs::symlink;
    let root = root();
    let outside = slot(&root, "outside", b"ready\n");
    symlink(&outside, root.join("profiles").join(FIRST)).unwrap();
    let second = slot(&root, SECOND, b"pending\n");
    symlink(outside.join("state"), second.join("storage-mode")).unwrap();
    assert!(list(&root, Layout::Desktop)
        .unwrap()
        .iter()
        .all(|slot| slot.state == State::Broken));
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    symlink(root.join("profiles"), other.join("profiles")).unwrap();
    assert!(list(&other, Layout::Android).is_err());
}
