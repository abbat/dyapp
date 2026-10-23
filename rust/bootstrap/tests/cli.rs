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
    command_locale(dir, args, "C")
}

fn command_locale(dir: &Path, args: &[&str], locale: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dyappd"))
        .env_clear()
        .env("LC_ALL", locale)
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

/// `status` with no node on the listen address: exit 3.
fn stopped_status(dir: &Path) -> String {
    let output = command(dir, &["status", "--listen", "127.0.0.1:1"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn status_reports_absent_and_populated_stores_without_creating_missing_databases() {
    let dir = directory();
    std::fs::write(dir.join("node.id"), "operator-peer\n").unwrap();
    assert_eq!(
        stopped_status(&dir),
        "node stopped\npeer_id operator-peer\nprofiles none\nenvelopes none\n\
         envelope_bytes none\nmedia_blobs none\nmedia_bytes none\npurged_blobs none\ndeny_entries none\n\
         cached_peers 0\nuptime_seconds unavailable\nrequests unavailable\n\
         wire_in_bytes unavailable\nwire_out_bytes unavailable\n"
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
    let output = stopped_status(&dir);
    for expected in [
        "profiles 1".to_owned(),
        "envelopes 1".to_owned(),
        format!("envelope_bytes {}", record.encoded_len()),
        "media_blobs 0".to_owned(),
        "media_bytes 0".to_owned(),
        "purged_blobs 0".to_owned(),
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
    assert!(stopped_status(&dir).contains("deny_entries 1\n"));

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
fn operator_diagnostics_follow_locale_without_changing_status_fields() {
    let dir = directory();
    let run = |args: &[&str]| command_locale(&dir, args, "ru_RU.UTF-8");
    let stderr = |output: Output| String::from_utf8(output.stderr).unwrap();
    assert!(stderr(run(&["--listen"])).contains("--listen требует значения"));
    assert!(stderr(run(&["unknown-command"])).contains("использование: dyappd"));
    assert!(stderr(run(&["id"])).contains("один раз запустите `dyappd keygen`"));
    assert!(stderr(run(&["deny", "add", "invalid"])).contains("не peer ID, SHA-256"));
    let entry = "192.0.2.0/24";
    assert_eq!(
        String::from_utf8(run(&["deny", "add", entry]).stdout).unwrap(),
        format!("{entry} добавлено\n")
    );
    assert_eq!(
        String::from_utf8(run(&["deny", "add", entry]).stdout).unwrap(),
        format!("{entry} уже в списке\n")
    );
    assert_eq!(
        String::from_utf8(run(&["deny", "remove", entry]).stdout).unwrap(),
        format!("{entry} удалено\n")
    );
    assert_eq!(
        String::from_utf8(run(&["deny", "remove", entry]).stdout).unwrap(),
        format!("{entry} нет в списке\n")
    );
    let status = String::from_utf8(run(&["status", "--listen", "127.0.0.1:1"]).stdout).unwrap();
    assert!(status.contains("node stopped\n"));
    assert!(status.contains("deny_entries 0\n"));
    assert!(stderr(command(&dir, &["--listen"])).contains("--listen needs a value"));
}

#[test]
fn client_argument_errors_follow_locale() {
    let dir = directory();
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["sign-profile", "00", "1"],
            "secret must be 32 bytes",
            "секрет должен быть длиной 32 байта",
        ),
        (
            &["sign-profile", "0", "1"],
            "odd-length hex",
            "нечётная длина шестнадцатеричной строки",
        ),
        (
            &["sign-profile", "zz", "1"],
            "bad hex",
            "неверная шестнадцатеричная строка",
        ),
        (
            &["media-sized", "invalid", "6291457", "00"],
            "blob exceeds 6 MiB",
            "размер файла превышает 6 МиБ",
        ),
        (
            &["media-sized", "invalid", "1", "00"],
            "seed must be 32 bytes",
            "начальное значение должно быть длиной 32 байта",
        ),
        (
            &["info", "invalid"],
            "bad address invalid",
            "неверный адрес invalid",
        ),
    ];
    for &(args, english, russian) in cases {
        let en = String::from_utf8(command_locale(&dir, args, "C").stderr).unwrap();
        let ru = String::from_utf8(command_locale(&dir, args, "ru_RU.UTF-8").stderr).unwrap();
        assert!(en.contains(english), "{args:?}: {en}");
        assert!(ru.contains(russian), "{args:?}: {ru}");
    }
    let en = String::from_utf8(command_locale(&dir, &["info", "invalid"], "C").stderr).unwrap();
    assert!(en.contains("node.example.org:33445"));
}

#[test]
fn project_authored_config_errors_follow_locale() {
    let dir = directory();
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["--missing.option", "1"],
            "unknown option --missing.option",
            "неизвестный параметр --missing.option",
        ),
        (
            &["--listen", "invalid"],
            "bad address invalid",
            "неверный адрес invalid",
        ),
        (
            &["--seeds", "invalid"],
            "bad seed invalid",
            "неверный seed invalid",
        ),
        (
            &["--turn.secret", "secret", "--turn.secret-file", "unused"],
            "turn.secret and turn.secret_file are both set",
            "одновременно заданы turn.secret и turn.secret_file",
        ),
    ];
    for &(args, english, russian) in cases {
        let en = String::from_utf8(command_locale(&dir, args, "C").stderr).unwrap();
        let ru = String::from_utf8(command_locale(&dir, args, "ru_RU.UTF-8").stderr).unwrap();
        assert!(en.contains(english), "{args:?}: {en}");
        assert!(ru.contains(russian), "{args:?}: {ru}");
    }
    let config = dir.join("removed.toml");
    std::fs::write(&config, "[storage]\nprofiles = \"old.db\"\n").unwrap();
    let args = ["--config", config.to_str().unwrap(), "status"];
    let en = String::from_utf8(command_locale(&dir, &args, "C").stderr).unwrap();
    let ru = String::from_utf8(command_locale(&dir, &args, "ru_RU.UTF-8").stderr).unwrap();
    assert!(en.contains("storage.profiles is removed"), "{en}");
    assert!(ru.contains("storage.profiles удалён"), "{ru}");
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
    assert!(stdout.contains("\npeer_id from-cwd\n"), "{stdout}");
}

#[test]
fn status_tells_a_running_node_and_its_traffic_from_a_stopped_one() {
    let dir = directory();
    let free_port = || {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
            .to_string()
    };
    let (listen, metrics) = (format!("127.0.0.1:{}", free_port()), free_port());
    let options = [
        "--network.id-pow-bits",
        "0",
        "--limits.min-free-mb",
        "0",
        "--listen",
        &listen,
        "--metrics.port",
        &metrics,
    ];
    let status = |extra: &[&str]| command(&dir, &[&["status"], &options[..], extra].concat());
    success(&dir, &[&["keygen"], &options[..]].concat());
    let blob = b"media CLI roundtrip";
    let hash = dyapp_identity::sha256(blob);
    let hash_hex: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    let media = dyapp_bootstrap::media::MediaStore::open(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(media
        .keep("test", 1, &[hash.to_vec()], now)
        .unwrap()
        .is_some());
    assert_eq!(
        media.put("test", blob, 1 << 20).unwrap(),
        dyapp_bootstrap::media::Put::Stored
    );
    drop(media);
    let mut node = Command::new(env!("CARGO_BIN_EXE_dyappd"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|path| ("LLVM_PROFILE_FILE", path)))
        .env("DYAPPD_ROLES", "[\"store\", \"media\"]")
        .arg("--storage.dir")
        .arg(&dir)
        .args(options)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut output = status(&[]);
    for _ in 0..60 {
        if output.status.success() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        output = status(&[]);
    }
    let running = String::from_utf8(output.stdout).unwrap();
    assert!(running.starts_with("node running\n"), "{running}");
    for name in [
        "uptime_seconds",
        "requests",
        "wire_in_bytes",
        "wire_out_bytes",
    ] {
        let line = running.lines().find(|l| l.starts_with(name)).unwrap();
        assert!(line[name.len() + 1..].parse::<u64>().is_ok(), "{line}");
    }
    // The node answered the earlier probes, so their requests are counted.
    assert!(!running.contains("\nrequests 0\n"), "{running}");

    let output = status(&["--metrics.port", "0"]);
    assert!(output.status.success());
    let unmetered = String::from_utf8(output.stdout).unwrap();
    assert!(unmetered.starts_with("node running\n"), "{unmetered}");
    assert!(
        unmetered.ends_with("wire_out_bytes unavailable\n"),
        "{unmetered}"
    );

    let fetched: serde_json::Value =
        serde_json::from_str(&success(&dir, &["media-get", &listen, &hash_hex])).unwrap();
    assert_eq!(fetched["status"], "STATUS_OK");
    assert_eq!(
        fetched["data"],
        blob.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );

    node.kill().unwrap();
    node.wait().unwrap();
    let output = status(&[]);
    assert_eq!(output.status.code(), Some(3));
    let stopped = String::from_utf8(output.stdout).unwrap();
    assert!(stopped.starts_with("node stopped\n"), "{stopped}");
    assert!(
        stopped.ends_with("wire_out_bytes unavailable\n"),
        "{stopped}"
    );
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
fn help_uses_the_message_locale_without_changing_commands() {
    let dir = directory();
    let help = |lc_all: &str, lc_messages: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_dyappd"))
            .env_clear()
            .env("LC_ALL", lc_all)
            .env("LC_MESSAGES", lc_messages)
            .env("LANG", "C")
            .current_dir(&dir)
            .arg("--help")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let ru = help("ru_RU.UTF-8", "C");
    assert!(ru.contains("использование: dyappd"));
    assert!(ru.contains("клиент: dyappd info <адрес>"));
    assert!(ru.contains("Значения по умолчанию:"));
    assert!(ru.contains("--storage.dir"));
    assert!(ru.contains("каталог всех данных узла"));
    assert!(help("", "ru_RU.UTF-8").contains("использование: dyappd"));
    let en = help("C", "ru_RU.UTF-8");
    assert!(en.contains("usage: dyappd"));
    assert!(en.contains("client: dyappd info <addr>"));
    assert!(!en.contains("использование:"));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn validation_errors_follow_locale_without_changing_option_names() {
    let dir = directory();
    for (locale, expected) in [
        (
            "C",
            "media.shard_threshold must be between 1 and 1048576 bytes",
        ),
        (
            "ru_RU.UTF-8",
            "media.shard_threshold: значение должно быть от 1 до 1048576 байт",
        ),
    ] {
        let output = command_locale(&dir, &["keygen", "--media.shard_threshold", "0"], locale);
        assert!(!output.status.success(), "{output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(expected), "{error}");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn key_errors_follow_locale_without_replacing_node_data() {
    let dir = directory();
    std::fs::write(dir.join("node.id"), "existing-node").unwrap();
    for (locale, expected) in [
        ("C", "exists: a new key is a new node"),
        (
            "ru_RU.UTF-8",
            "уже существует: новый ключ создаёт новый узел",
        ),
    ] {
        let output = command_locale(&dir, &["keygen"], locale);
        assert!(!output.status.success(), "{output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(expected), "{error}");
        assert_eq!(
            std::fs::read_to_string(dir.join("node.id")).unwrap(),
            "existing-node"
        );
    }
}

#[test]
fn network_marker_errors_follow_locale_without_rewriting_marker() {
    let dir = directory();
    let marker = vec![b'x'; 65];
    std::fs::write(dir.join("network"), &marker).unwrap();
    for (locale, expected) in [
        ("C", "longer than a network name"),
        ("ru_RU.UTF-8", "имя сети слишком длинное"),
    ] {
        let output = command_locale(&dir, &["keygen"], locale);
        assert!(!output.status.success(), "{output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(expected), "{error}");
        assert_eq!(std::fs::read(dir.join("network")).unwrap(), marker);
    }
}

#[test]
fn malformed_toml_never_echoes_secret_values() {
    let dir = directory();
    let config = dir.join("bad.toml");
    std::fs::write(&config, "[turn]\nsecret = \"CANARY_SECRET_VALUE\" ???\n").unwrap();
    for locale in ["C", "ru_RU.UTF-8"] {
        let output = command_locale(
            &dir,
            &["status", "--config", config.to_str().unwrap()],
            locale,
        );
        assert!(!output.status.success(), "{output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(config.to_str().unwrap()), "{error}");
        assert!(
            error.contains("TOML parse error at line 2, column"),
            "{error}"
        );
        assert!(!error.contains("CANARY_SECRET_VALUE"), "{error}");
    }
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
