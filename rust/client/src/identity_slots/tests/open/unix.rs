use super::{key_file, recovery, root, slot, Reader, FIRST};
use crate::identity_slots::{resolve, Layout, RecoveryReason};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn a_raw_key_readable_by_other_users_is_not_opened() {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    fs::write(dir.join("storage-mode"), b"file\n").unwrap();
    key_file(&dir, &[7; 32]);
    fs::set_permissions(dir.join("identity.key"), fs::Permissions::from_mode(0o644)).unwrap();
    recovery(
        resolve(&root, Layout::Desktop, &Reader::default()).unwrap(),
        RecoveryReason::Unavailable,
    );
    assert_eq!(fs::read(dir.join("identity.key")).unwrap(), [7; 32]);
}

#[test]
fn symlinked_wrapped_key_never_reaches_the_platform_decoder() {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    key_file(&outside, b"old wrapped fixture");
    symlink(outside.join("identity.key"), dir.join("identity.key")).unwrap();
    let keys = Reader {
        present: true,
        ..Reader::default()
    };
    recovery(
        resolve(&root, Layout::Android, &keys).unwrap(),
        RecoveryReason::Unavailable,
    );
    assert_eq!(keys.calls.get(), 0);
}
