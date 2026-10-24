use super::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "/tmp/ai/dyapp-config-proptest-regressions.txt"
            )
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_set_preserves_values_and_unrelated_fields(
        path in proptest::collection::vec("[a-z]{1,8}", 1..5),
        value in any::<i64>(),
    ) {
        let mut table = toml::Table::new();
        table.insert("__sentinel".into(), toml::Value::Boolean(true));
        set(&mut table, &path, toml::Value::Integer(value), "generated").unwrap();
        let root = toml::Value::Table(table);
        let actual = path.iter().try_fold(&root, |node, key| node.get(key));
        prop_assert_eq!(actual, Some(&toml::Value::Integer(value)));
        prop_assert_eq!(root.get("__sentinel"), Some(&toml::Value::Boolean(true)));
    }

    #[test]
    fn prop_config_precedence(
        file_value in any::<u32>(),
        env_value in any::<u32>(),
        cli_value in any::<u32>(),
        use_env in any::<bool>(),
        use_cli in any::<bool>(),
    ) {
        let dir = temp_dir();
        fs::create_dir_all(&dir)?;
        let file = dir.join("config.toml");
        fs::write(&file, format!("[limits]\nrequests_per_second = {file_value}\n"))?;
        let env = if use_env {
            vec![("DYAPPD_LIMITS_REQUESTS_PER_SECOND".into(), env_value.to_string())]
        } else {
            vec![]
        };
        let options = if use_cli {
            vec![("limits.requests-per-second".into(), cli_value.to_string())]
        } else {
            vec![]
        };
        let (config, ignored) = NodeConfig::load(Some(&file), env, &options).unwrap();
        let expected = if use_cli { cli_value } else if use_env { env_value } else { file_value };
        prop_assert_eq!(config.limits.requests_per_second, expected);
        prop_assert!(ignored.is_empty());
    }

    #[test]
    fn prop_cli_strings_remain_literal(
        text in prop_oneof![Just("00123".to_owned()), Just("a=b,c=d".to_owned()), ".{0,80}"],
    ) {
        let (config, _) = NodeConfig::load(
            None,
            Vec::<(String, String)>::new(),
            &[("turn.secret".into(), text.clone())],
        ).unwrap();
        prop_assert_eq!(config.turn.secret.expose(), text);
    }

    #[test]
    fn prop_section_collisions_and_unknown_options_are_errors(
        key in "[a-z]{1,12}",
        value in any::<i64>(),
    ) {
        let mut table = toml::Table::new();
        table.insert("section".into(), toml::Value::Integer(value));
        let error = set(&mut table, &["section".into(), key.clone()], toml::Value::Integer(1), "generated")
            .unwrap_err().to_string();
        prop_assert!(error.contains("section is not a section"));
        let option = format!("unknown-{key}");
        let error = NodeConfig::load(
            None, Vec::<(String, String)>::new(), &[(option.clone(), value.to_string())]
        ).unwrap_err().to_string();
        let expected = format!("unknown option --{option}");
        prop_assert!(error.contains(&expected));
    }

    #[test]
    fn prop_media_threshold_validates_documented_range(
        threshold in prop_oneof![
            Just(0usize), Just(1usize), Just(1usize << 20),
            Just((1usize << 20) + 1), Just(usize::MAX), 0usize..=(2usize << 20)
        ],
    ) {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.media.shard_threshold = threshold;
        prop_assert_eq!(config.validate().is_ok(), (1..=1048576).contains(&threshold));
    }
}

#[test]
fn search_rejects_invalid_puzzle_bounds() {
    let mut search = Search {
        puzzle_wait_ms: 0,
        ..Search::default()
    };
    assert!(search
        .validate()
        .unwrap_err()
        .to_string()
        .contains("search.puzzle_wait_ms must be at least 1"));
    search.puzzle_wait_ms = 1;
    search.max_puzzle_bits = 33;
    assert!(search
        .validate()
        .unwrap_err()
        .to_string()
        .contains("max_puzzle_bits 12..=32"));
}

pub(super) fn temp_dir() -> PathBuf {
    PathBuf::from(format!("/tmp/ai/test-config-{}", uuid::Uuid::new_v4()))
}

#[test]
fn storage_lock_refuses_a_second_holder_until_dropped() {
    let storage = StorageConfig {
        dir: temp_dir(),
        ..StorageConfig::default()
    };
    fs::create_dir_all(&storage.dir).unwrap();
    let held = storage.lock().unwrap();
    let error = storage.lock().unwrap_err().to_string();
    assert!(error.contains("dyappd.lock"), "{error}");
    drop(held);
    storage.lock().unwrap();
    fs::remove_dir_all(&storage.dir).unwrap();
}

#[test]
fn file_then_env_overrides_and_unknown_keys() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("node.toml");
    fs::write(
        &file,
        "listen = [\"127.0.0.1:7071\"]\nfuture_key = 1\n\
         [limits]\nmessage_ttl_hours = 48\n",
    )
    .unwrap();
    let env = [
        ("DYAPPD_LIMITS_MESSAGE_TTL_HOURS".into(), "72".into()),
        ("DYAPPD_STORAGE_DIR".into(), "/srv/node".into()),
        ("DYAPPD_SEEDS".into(), "[\"seed.example:7070\"]".into()),
        ("DYAPPD_LIMITS_PAST_KEY".into(), "1".into()),
        ("OTHER".into(), "x".into()),
    ];
    let (config, ignored) = NodeConfig::load(Some(&file), env, &[]).unwrap();
    assert_eq!(config.listen, ["127.0.0.1:7071"]);
    assert_eq!(config.seeds, ["seed.example:7070"]);
    assert_eq!(config.limits.message_ttl_hours, 72);
    assert_eq!(config.limits.requests_per_second, 100);
    assert_eq!(config.storage.dir, PathBuf::from("/srv/node"));
    assert_eq!(
        config.storage.messages_path(),
        PathBuf::from("/srv/node/messages.db")
    );
    assert_eq!(ignored, ["future_key", "limits.past_key"]);
    // A variable name ends its section at the first `_`.
    let defaults = toml::Value::try_from(NodeConfig::default()).unwrap();
    for (name, value) in defaults.as_table().unwrap() {
        assert!(!(value.is_table() && name.contains('_')), "section {name}");
    }
}

#[test]
fn options_override_env() {
    assert_eq!(
        NodeConfig::default().listen,
        ["[::1]:33445", "127.0.0.1:33445"]
    );
    let env = [("DYAPPD_LIMITS_MAX_CONNECTIONS".into(), "5".into())];
    let options = [
        ("limits.max-connections", "7"),
        ("listen", "[::]:7070"),
        ("listen", "0.0.0.0:7070"),
        ("turn.secret", "123"),
        ("network.share-deny-list", "true"),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()));
    let (config, _) = NodeConfig::load(None, env, &options).unwrap();
    assert_eq!(config.limits.max_connections, 7);
    assert_eq!(config.listen, ["[::]:7070", "0.0.0.0:7070"]);
    assert_eq!(config.turn.secret.expose(), "123");
    assert!(config.network.share_deny_list);
    for option in ["limits.max-connection", "limits", "x"] {
        let options = [(option.to_owned(), "1".to_owned())];
        assert!(NodeConfig::load(None, [], &options).is_err());
    }
}

#[test]
fn turn_secret_file() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("turn.secret");
    let path = file.display().to_string();
    let load = |content: &str, secret: Option<&str>| {
        fs::write(&file, content).unwrap();
        let mut env = vec![("DYAPPD_TURN_SECRET_FILE".to_owned(), path.clone())];
        env.extend(secret.map(|s| ("DYAPPD_TURN_SECRET".to_owned(), s.to_owned())));
        let (mut config, _) = NodeConfig::load(None, env, &[]).unwrap();
        config
            .resolve_secrets()
            .map(|()| config.turn.secret.expose().to_owned())
    };
    assert_eq!(load("s3cret\n", None).unwrap(), "s3cret");
    assert_eq!(load("s3cret\n\n", None).unwrap(), "s3cret\n");
    assert!(load("", None).unwrap_err().to_string().contains(&path));
    let both = load("s3cret", Some("other")).unwrap_err().to_string();
    assert_eq!(both, "turn.secret and turn.secret_file are both set");
    fs::remove_dir_all(&dir).unwrap();
    let options = [("turn.secret-file".to_owned(), path.clone())];
    let (mut config, _) = NodeConfig::load(None, [], &options).unwrap();
    assert!(config
        .resolve_secrets()
        .unwrap_err()
        .to_string()
        .contains(&path));
    let mut config = NodeConfig::default();
    config.resolve_secrets().unwrap();
    assert!(config.turn.secret.expose().is_empty());
}

#[test]
fn deny_dial_defaults_overrides_and_validation() {
    let mut config = NodeConfig::default();
    assert_eq!(
        config.network.deny_dial,
        [
            "169.254.0.0/16",
            "fe80::/10",
            "fd00:ec2::254/128",
            "100.100.100.200/32"
        ]
    );
    let filter = config.network.dial_filter().unwrap();
    assert!(filter.denies(&"/ip4/169.254.169.254/tcp/80".parse().unwrap()));
    assert!(!filter.denies(&"/ip4/10.0.0.1/tcp/80".parse().unwrap()));
    assert!(options_help().contains("--network.deny-dial"));

    let env = [("DYAPPD_NETWORK_DENY_DIAL".into(), "[\"10.0.0.0/8\"]".into())];
    let (loaded, _) = NodeConfig::load(None, env, &[]).unwrap();
    assert_eq!(loaded.network.deny_dial, ["10.0.0.0/8"]);
    let options =
        ["10.0.0.0/8", "fc00::/7"].map(|net| ("network.deny-dial".to_owned(), net.into()));
    let (loaded, _) = NodeConfig::load(None, [], &options).unwrap();
    assert_eq!(loaded.network.deny_dial, ["10.0.0.0/8", "fc00::/7"]);

    config.storage.dir = temp_dir();
    config.network.deny_dial = vec![];
    config.validate().unwrap();
    config.network.deny_dial = vec!["10.0.0.0/8".into(), "nonsense".into()];
    let error = config.validate().unwrap_err().to_string();
    assert_eq!(error, "network.deny_dial: invalid CIDR \"nonsense\"");
}

/// The packaged file, and so `--help`, lists every key with the default the code uses.
#[test]
fn reference_matches_defaults() {
    let mut table = toml::Table::new();
    for (section, key, default, _) in documented() {
        let path: Vec<String> = format!("{section}{key}")
            .split('.')
            .map(Into::into)
            .collect();
        set(&mut table, &path, toml_value(default), key).unwrap();
    }
    let defaults = toml::Value::try_from(NodeConfig::default()).unwrap();
    assert_eq!(toml::Value::Table(table), defaults);
    assert!(options_help().contains("--limits.max-connections"));
}

#[test]
fn russian_help_covers_every_documented_option() {
    let documented: std::collections::BTreeSet<_> = documented()
        .into_iter()
        .map(|(section, key, _, _)| format!("{section}{key}"))
        .collect();
    let translated: std::collections::BTreeSet<_> = ru::DESCRIPTIONS
        .iter()
        .map(|(key, description)| {
            assert!(!description.trim().is_empty(), "{key}");
            key.to_string()
        })
        .collect();
    assert_eq!(translated.len(), ru::DESCRIPTIONS.len());
    assert_eq!(translated, documented);
    assert!(options_help_russian().contains("--storage.dir"));
    assert!(options_help_russian().contains("каталог всех данных узла"));
}

#[test]
fn node_key_refuses_group_other_bits_without_repairing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.network.id_pow_bits = 4;
    config.validate().unwrap();
    let peer = config.keygen(None).unwrap();
    let path = config.storage.key_path();
    for mode in [0o640, 0o604, 0o601] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        let error = config.node_key().unwrap_err().to_string();
        assert!(error.contains("chmod 600"), "{error}");
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, mode);
    }
    for mode in [0o600, 0o400] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(config.node_key().unwrap().public().to_peer_id(), peer);
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, mode);
    }
}

#[test]
fn node_key_is_bound_to_the_data() {
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.network.id_pow_bits = 4;
    config.validate().unwrap();
    config.node_key().unwrap_err();
    let peer = config.keygen(None).unwrap();
    config.keygen(None).unwrap_err();
    let key = config.node_key().unwrap();
    assert_eq!(key.public().to_peer_id(), peer);
    let mode = fs::metadata(config.storage.key_path()).unwrap().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        config.node_key().unwrap().public(),
        key.public(),
        "the key is reused"
    );

    // A replaced key does not match the data; a deleted one is not silently recreated.
    let other = Keypair::generate_ed25519().to_protobuf_encoding().unwrap();
    fs::write(config.storage.key_path(), other).unwrap();
    config.node_key().unwrap_err();
    fs::remove_file(config.storage.key_path()).unwrap();
    config.node_key().unwrap_err();
    // node.id is left: keygen does not replace the node.
    config.keygen(None).unwrap_err();

    // A key without the proof of work is refused.
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.network.id_pow_bits = 0;
    config.validate().unwrap();
    config.keygen(None).unwrap();
    config.network.id_pow_bits = 32;
    config.node_key().unwrap_err();
}

#[test]
fn key_source_is_independent_of_its_protection() {
    use std::os::unix::fs::PermissionsExt;
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.network.id_pow_bits = 4;
    config.validate().unwrap();
    // keygen to a writer leaves no key file: another tool seals what it gets.
    let mut sealed = Vec::new();
    let peer = config.keygen(Some(&mut sealed)).unwrap();
    assert!(!config.storage.key_path().exists());
    config.load_key(None).unwrap_err();

    // A systemd credential: readable through an ACL, so its mode shows group bits.
    let credentials = temp_dir();
    fs::create_dir_all(&credentials).unwrap();
    let credential = credentials.join("node.key");
    fs::write(&credential, &sealed).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o440)).unwrap();
    let key = config.load_key(Some(&credentials)).unwrap();
    assert_eq!(key.public().to_peer_id(), peer);

    // An explicit storage.key wins over the credential, and its mode is checked.
    config.storage.key = credentials.join("plain.key");
    fs::write(&config.storage.key, &sealed).unwrap();
    fs::set_permissions(&config.storage.key, fs::Permissions::from_mode(0o640)).unwrap();
    let error = config.load_key(Some(&credentials)).unwrap_err().to_string();
    assert!(
        error.contains("plain.key") && error.contains("chmod 600"),
        "{error}"
    );
    fs::set_permissions(&config.storage.key, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        config.load_key(Some(&credentials)).unwrap().public(),
        key.public()
    );

    // keygen writes a plain key to storage.key, outside storage.dir.
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.storage.key = credentials.join("node2.key");
    config.network.id_pow_bits = 4;
    config.validate().unwrap();
    let peer = config.keygen(None).unwrap();
    assert!(!config.storage.dir.join("node.key").exists());
    let mode = fs::metadata(&config.storage.key).unwrap().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(config.load_key(None).unwrap().public().to_peer_id(), peer);
}

fn named(name: &str) -> NodeConfig {
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    config.network.name = name.into();
    config.network.id_pow_bits = 0;
    config.validate().unwrap();
    config
}

fn marker(config: &NodeConfig) -> Vec<u8> {
    fs::read(config.storage.dir.join("network")).unwrap()
}

#[test]
fn keygen_and_start_bind_storage_to_one_network() {
    // keygen binds a fresh directory to the configured name.
    let mut config = named("city");
    config.keygen(None).unwrap();
    assert_eq!(marker(&config), b"city");
    config.bind_network().unwrap();
    config.network.name = "other".into();
    let error = config.bind_network().unwrap_err().to_string();
    assert!(
        error.contains("\"city\"") && error.contains("\"other\""),
        "{error}"
    );
    config.network.name = String::new();
    config.bind_network().unwrap_err();
    assert_eq!(marker(&config), b"city");

    // A first start without data takes the configured name, the public one included.
    for name in ["city", ""] {
        let config = named(name);
        config.bind_network().unwrap();
        assert_eq!(marker(&config), name.as_bytes());
    }

    // keygen in a directory of another network writes neither key nor id.
    let mut config = named("city");
    config.bind_network().unwrap();
    config.network.name = "other".into();
    config.keygen(None).unwrap_err();
    assert!(!config.storage.key_path().exists());
    assert!(!config.storage.dir.join("node.id").exists());
    assert_eq!(marker(&config), b"city");
}

#[test]
fn data_without_a_marker_belongs_to_the_public_network() {
    let mut data: Vec<String> = ["profiles.db", "profiles-idx.db", "messages.db", "media.db"]
        .into_iter()
        .flat_map(|store| ["", "-wal", "-shm", "-journal"].map(|suffix| format!("{store}{suffix}")))
        .collect();
    data.extend(["data/".into(), "media/".into()]);
    for name in data {
        let mut config = named("city");
        let path = config.storage.dir.join(&name);
        if name.ends_with('/') {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, b"").unwrap();
        }
        let error = config.bind_network().unwrap_err().to_string();
        assert!(
            error.contains("storage belongs to the public network"),
            "{name}: {error}"
        );
        assert_eq!(marker(&config), b"", "{name}");
        config.keygen(None).unwrap_err();
        config.network.name = String::new();
        config.bind_network().unwrap();
    }

    // The deny list, peer caches and a key alone are not data.
    let config = named("city");
    for path in [
        config.storage.deny_path(),
        config.storage.peers_path(),
        config.storage.anchors_path(),
        config.storage.key_path(),
    ] {
        fs::write(path, b"").unwrap();
    }
    config.bind_network().unwrap();
    assert_eq!(marker(&config), b"city");
}

#[test]
fn malformed_markers_are_refused_and_kept() {
    for bad in [
        vec![b'a'; 65],
        vec![0xff],
        b"bad\nname".to_vec(),
        b" city".to_vec(),
    ] {
        for name in ["", "city"] {
            let config = named(name);
            fs::write(config.storage.dir.join("network"), &bad).unwrap();
            config.bind_network().unwrap_err();
            config.keygen(None).unwrap_err();
            assert_eq!(marker(&config), bad);
            assert!(!config.storage.key_path().exists());
        }
    }
    // A directory in place of the marker is an inspection error, not a missing marker.
    let config = named("city");
    fs::create_dir(config.storage.dir.join("network")).unwrap();
    config.bind_network().unwrap_err();
}
