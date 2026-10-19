use super::tests::temp_dir;
use super::*;

#[test]
fn search_limits_are_checked() {
    let search = |change: fn(&mut Search)| {
        let mut config = NodeConfig::default();
        change(&mut config.search);
        config.search.validate()
    };
    assert!(search(|_| {}).is_ok());
    assert!(search(|s| s.max_profile_bytes = 0).is_err());
    assert!(search(|s| s.max_profile_bytes = crate::service::SEARCH_ACCEPT_BYTES + 1).is_err());
    assert!(search(|s| s.max_profile_bytes = crate::service::SEARCH_ACCEPT_BYTES).is_ok());
    assert!(search(|s| s.max_scan_rows = 0).is_err());
    assert!(search(|s| s.topics = vec![16]).is_err());
    assert!(search(|s| s.max_bytes = 0).is_err());
    assert_eq!(Search::default().max_profile_bytes, 64 << 10);
    assert_eq!(Search::default().min_profile_pow_bits, 11);
    assert!(search(|s| s.min_profile_pow_bits = 20).is_ok());
    assert!(search(|s| s.min_profile_pow_bits = 21).is_err());
    assert!(Search::default().keeps(15));
    let one = Search {
        topics: vec![3],
        ..Search::default()
    };
    assert!(one.keeps(3) && !one.keeps(4));
}

#[test]
fn incoming_group_and_coarse_settings_validate_boundaries() {
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    assert_eq!(config.limits.max_connections_per_ip_group, 32);
    assert_eq!(config.limits.coarse_ipv4_prefix, 16);
    assert_eq!(config.limits.coarse_ipv6_prefix, 32);
    assert!(config.validate().is_ok());

    config.limits.max_connections_per_ip_group = 0;
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("max_connections_per_ip_group"));
    config.limits.max_connections_per_ip_group = 1;
    for (v4, v6) in [(0, 0), (32, 128), (16, 32)] {
        config.limits.coarse_ipv4_prefix = v4;
        config.limits.coarse_ipv6_prefix = v6;
        // Coarse masks are independent: original endpoint metadata is needed at runtime.
        config.limits.ipv4_prefix = 0;
        config.limits.ipv6_prefix = 0;
        assert!(config.validate().is_ok());
    }
    config.limits.coarse_ipv4_prefix = 33;
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("coarse_ipv4_prefix"));
    config.limits.coarse_ipv4_prefix = 16;
    config.limits.coarse_ipv6_prefix = 129;
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("coarse_ipv6_prefix"));
    config.limits.coarse_ipv6_prefix = 32;
    for rate in [1, u32::MAX / 10] {
        config.limits.ip_group_requests_per_second = rate;
        assert!(config.validate().is_ok());
    }
    for rate in [0, u32::MAX / 10 + 1, u32::MAX] {
        config.limits.ip_group_requests_per_second = rate;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("ip_group_requests_per_second"));
    }
}

#[test]
fn invalid_config_fails() {
    let invalid = |edit: fn(&mut NodeConfig)| {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        edit(&mut config);
        config.validate().is_err()
    };
    assert!(!invalid(|_| {}));
    assert!(invalid(|c| c.media.shard_threshold = (1 << 20) + 1));
    assert!(invalid(|c| c.media.shard_threshold = 0));
    assert!(!invalid(|c| c.media.shard_threshold = 1));
    assert!(invalid(|c| c.listen = vec!["not an address".into()]));
    assert!(invalid(|c| c.listen = vec!["/ip4/0.0.0.0/tcp/7070".into()]));
    assert!(invalid(|c| c.external = vec!["[2001:db8::1]".into()]));
    assert_eq!(
        transports("[2001:db8::1]:7070")
            .unwrap()
            .map(|a| a.to_string()),
        [
            "/ip6/2001:db8::1/tcp/7070",
            "/ip6/2001:db8::1/udp/7070/quic-v1"
        ]
    );
    assert!(invalid(|c| c.seeds = vec!["seed.example".into()]));
    assert!(invalid(
        |c| c.seeds = vec!["seed.example:7070/not-a-peer-id".into()]
    ));
    assert!(invalid(
        |c| c.seeds = vec!["/dns/seed.example/tcp/7070".into()]
    ));
    assert!(invalid(|c| c.seeds = vec!["[seed.example]:7070".into()]));
    assert_eq!(
        seed("seed.example:7070").unwrap().map(|a| a.to_string()),
        [
            "/dns/seed.example/tcp/7070",
            "/dns/seed.example/udp/7070/quic-v1"
        ]
    );
    assert_eq!(
        seed("[2001:db8::7]:7070").unwrap(),
        transports("[2001:db8::7]:7070").unwrap()
    );
    fn turn(c: &mut NodeConfig) {
        c.roles = vec![Role::Store, Role::Turn];
        c.turn.secret = "s".into();
        c.turn.urls = vec!["turn:relay.example:3478".into()];
    }
    assert!(!invalid(turn));
    assert!(invalid(|c| {
        turn(c);
        c.roles = vec![Role::Turn];
    }));
    assert!(invalid(|c| {
        turn(c);
        c.turn.secret = Default::default();
    }));
    assert!(invalid(|c| {
        turn(c);
        c.turn.credential_minutes = 0;
    }));
    assert!(!invalid(|c| {
        turn(c);
        c.turn.credential_minutes = 1;
    }));
    assert!(invalid(|c| {
        turn(c);
        c.turn.urls = vec!["relay.example:3478".into()];
    }));
    assert!(invalid(|c| c.roles = vec![Role::Media]));
    assert!(!invalid(|c| c.roles = vec![Role::Store, Role::Media]));
    assert!(invalid(|c| c.roles = vec![Role::Search]));
    assert!(!invalid(|c| c.roles = vec![Role::Store, Role::Search]));
    fn subset(c: &mut NodeConfig) {
        c.roles = vec![Role::Store, Role::Search];
        c.search.topics = vec![3];
    }
    assert!(invalid(subset));
    assert!(!invalid(|c| {
        subset(c);
        c.network.name = "test".into();
    }));
    assert!(invalid(|c| c.limits.message_ttl_hours = 0));
    assert!(invalid(|c| c.limits.attachment_retention_hours = 0));
    assert!(invalid(|c| c.limits.profile_ttl_days = 0));
    assert!(invalid(|c| c.maintenance.io_mb_per_s = 0));
    assert!(invalid(|c| c.storage.dir = "/proc/dyappd".into()));
    let roles = [("DYAPPD__ROLES".into(), "[\"x\"]".into())];
    assert!(NodeConfig::load(None, roles, &[]).is_err());
    let removed = [("DYAPPD__STORAGE__MEDIA".into(), "/big/media".into())];
    assert!(NodeConfig::load(None, removed, &[]).is_err());
}

#[test]
fn network_names_validate_without_normalization_and_load_as_literal_strings() {
    let mut config = NodeConfig::default();
    config.storage.dir = temp_dir();
    assert!(config.network.name.is_empty());
    for name in ["", "City", "city", "a b", "00123", &"a".repeat(64)] {
        config.network.name = name.into();
        config.validate().unwrap();
        assert_eq!(config.network.name, name);
    }
    for name in [" a", "a ", "a\nb", "сеть", &"a".repeat(65)] {
        config.network.name = name.into();
        let expected = dyapp_p2p_net::Protocols::new(name).unwrap_err().to_string();
        assert_eq!(config.validate().unwrap_err().to_string(), expected);
    }
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("network.toml");
    fs::write(&path, "[network]\nname = \"file\"\n").unwrap();
    let load = |env: Vec<(String, String)>, options: Vec<(String, String)>| {
        let (config, ignored) = NodeConfig::load(Some(&path), env, &options).unwrap();
        assert!(ignored.is_empty());
        config.network.name
    };
    assert_eq!(load(vec![], vec![]), "file");
    assert_eq!(
        load(vec![("DYAPPD__NETWORK__NAME".into(), "env".into())], vec![]),
        "env"
    );
    assert_eq!(
        load(
            vec![("DYAPPD__NETWORK__NAME".into(), "env".into())],
            vec![("network.name".into(), "00123".into())]
        ),
        "00123"
    );
    assert!(options_help().contains("--network.name"));
}
