use crate::identity_slots::{write::select_with, SelectionDurability};
use std::fs;

#[test]
fn failed_directory_sync_reports_committed_selection_with_unconfirmed_durability() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let root = test_dir.to_path_buf();
    let pointer = root.join("active-profile");
    fs::write(&pointer, b"old\n").unwrap();
    let result = select_with(&pointer, b"new\n", |_| {
        assert_eq!(fs::read(&pointer).unwrap(), b"new\n");
        Err("injected directory sync failure".into())
    })
    .unwrap();
    assert_eq!(
        result,
        SelectionDurability::Unconfirmed {
            detail: "injected directory sync failure".into()
        }
    );
    assert_eq!(fs::read(pointer).unwrap(), b"new\n");
}

#[test]
fn precommit_failure_keeps_old_selection_and_never_runs_postcommit_callback() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let root = test_dir.to_path_buf();
    let pointer = root.join("active-profile");
    fs::write(&pointer, b"old\n").unwrap();
    fs::write(root.join("active-profile.tmp"), b"stranded").unwrap();
    assert!(select_with(&pointer, b"new\n", |_| panic!("rename never committed")).is_err());
    assert_eq!(fs::read(pointer).unwrap(), b"old\n");
}
