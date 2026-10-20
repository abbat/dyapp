//! Operator CLI contracts run against the real binary and disposable stores.
use dyapp_bootstrap::deny::{self, DenyStore};
use dyapp_bootstrap::{storage::NewEnvelope, BootstrapStore};
use dyapp_identity::{Domain, Identity};
use dyapp_p2p_net::proto;
use prost::Message;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn directory() -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/ai/test-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn command(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dyappd"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|path| ("LLVM_PROFILE_FILE", path)))
        .current_dir(dir)
        .arg("--storage.dir")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
}

fn success(dir: &Path, args: &[&str]) -> String {
    let output = command(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn status_reports_absent_and_populated_stores_without_creating_missing_databases() {
    let dir = directory();
    std::fs::write(dir.join("node.id"), "operator-peer\n").unwrap();
    assert_eq!(
        success(&dir, &["status"]),
        "peer_id operator-peer\nprofiles none\nenvelopes none\nenvelope_bytes none\n\
         media_blobs none\nmedia_bytes none\ndeny_entries none\ncached_peers 0\n"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert_eq!(success(&dir, &["id"]), "operator-peer\n");
    let (v6, v4) = ("[2001:db8::7]:7070", "192.0.2.7:7070");
    assert_eq!(
        success(&dir, &["id", "--external", v6, "--external", v4]),
        "operator-peer\n[2001:db8::7]:7070/operator-peer\n192.0.2.7:7070/operator-peer\n"
    );

    let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
    let owner = Identity::generate();
    store
        .put_profile(
            &dyapp_profile::Profile {
                version: 1,
                age: 35,
                ..Default::default()
            }
            .sign(&owner),
        )
        .unwrap();
    let record = owner.sign(Domain::Envelope, vec![1; 7]);
    let envelope = NewEnvelope::new(&[2; 32], &[3; 16], &record, false, i64::MAX);
    store.put_envelope(&envelope, 1 << 20, None).unwrap();
    let media = dyapp_bootstrap::media::MediaStore::open(&dir).unwrap();
    let output = success(&dir, &["status"]);
    for expected in [
        "profiles 1".to_owned(),
        "envelopes 1".to_owned(),
        format!("envelope_bytes {}", record.encoded_len()),
        "media_blobs 0".to_owned(),
        "media_bytes 0".to_owned(),
        "deny_entries none".to_owned(),
    ] {
        assert!(output.lines().any(|line| line == expected), "{output}");
    }
    assert_eq!(store.list_profiles(0, 10).unwrap().len(), 1);
    assert_eq!(
        store.fetch_envelopes(&[2; 32], 10, 1 << 20).unwrap().0,
        [record]
    );
    assert!(!dir.join("deny.db").exists());
    drop(media);
}

#[test]
fn deny_cli_add_is_idempotent_and_received_lists_are_not_locally_applied() {
    let dir = directory();
    let entry = "192.0.2.0/24";
    assert_eq!(
        success(&dir, &["deny", "add", entry, "operator", "note"]),
        format!("{entry} added\n")
    );
    assert_eq!(
        success(&dir, &["deny", "add", entry, "replacement"]),
        format!("{entry} already listed\n")
    );
    let listed = success(&dir, &["deny", "list"]);
    assert_eq!(listed.lines().count(), 1);
    assert!(listed.starts_with(&format!("{entry}\t")));
    assert!(listed.trim_end().ends_with("\toperator note"));
    assert!(success(&dir, &["status"]).contains("deny_entries 1\n"));

    let remote = Identity::generate();
    let peer = remote.peer_id().to_string();
    let record = deny::sign(&remote, &["198.51.100.0/24".to_owned()].into());
    let list = proto::DenyList::decode(record.payload.as_slice()).unwrap();
    let store = DenyStore::open(&dir.join("deny.db")).unwrap();
    assert!(store.keep_received(&peer, list.time, &record).unwrap());
    let received = success(&dir, &["deny", "received"]);
    assert!(received.starts_with(&format!("{peer}\t")));
    assert!(received.contains("\n\t198.51.100.0/24\n"));
    assert_eq!(store.entries().unwrap(), [entry.to_owned()].into());

    for args in [
        vec!["deny"],
        vec!["deny", "add", "invalid"],
        vec!["unknown-command"],
    ] {
        assert!(!command(&dir, &args).status.success(), "{args:?}");
    }
}

#[test]
fn without_config_the_working_directory_file_is_read() {
    let (cwd, store) = (directory(), directory());
    std::fs::write(store.join("node.id"), "from-cwd\n").unwrap();
    let toml = format!("[storage]\ndir = {:?}\n", store.to_str().unwrap());
    std::fs::write(cwd.join("dyappd.toml"), toml).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dyappd"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|path| ("LLVM_PROFILE_FILE", path)))
        .current_dir(&cwd)
        .arg("status")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("peer_id from-cwd\n"), "{stdout}");
}

#[test]
fn client_commands_print_json_without_a_usable_storage_dir() {
    let dir = directory();
    let args = ["device-key", "--storage.dir", "/nonexistent/dyappd"];
    let reply: serde_json::Value = serde_json::from_str(&success(&dir, &args)).unwrap();
    assert_eq!(reply["mailbox"].as_str().map(str::len), Some(64));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn help_and_incomplete_options_do_not_create_stores() {
    let dir = directory();
    let help = success(&dir, &["--help"]);
    assert!(help.contains("usage: dyappd"));
    assert!(help.contains("--storage.dir"));
    let output = command(&dir, &["--listen"]);
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("--listen needs a value"));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}
