use super::*;

fn ascii(text: &str) -> Vec<String> {
    hosts(text).into_iter().map(|host| host.ascii).collect()
}

#[test]
fn desktop_smoke_uses_real_userinfo_target_and_punycode() {
    assert_eq!(
        ascii("See https://bücher.example/path and https://paypal.com@evil.example/login"),
        ["xn--bcher-kva.example", "evil.example"]
    );
    assert_eq!(hosts("javascript:alert(1)"), Vec::new());
    assert_eq!(
        ascii("[trusted.example](https://evil.example/login)"),
        ["evil.example"]
    );
}

#[test]
fn repeated_and_equivalent_hosts_keep_first_occurrence_order() {
    assert_eq!(
        ascii(
            "HTTPS://BÜCHER.example:443/a https://xn--bcher-kva.example/b \
               http://EXAMPLE.test:80, https://example.test:8443! https://example.test/path"
        ),
        ["xn--bcher-kva.example", "example.test", "example.test:8443"]
    );
}

#[test]
fn unicode_spelling_is_additional_to_the_required_ascii_destination() {
    assert_eq!(
        hosts("https://bücher.example:8443/path"),
        [LinkHost {
            ascii: "xn--bcher-kva.example:8443".into(),
            unicode: Some("bücher.example:8443".into()),
        }]
    );
    assert_eq!(hosts("https://example.test")[0].unicode, None);
}

#[test]
fn ip_addresses_and_nondefault_ports_follow_url_canonicalization() {
    assert_eq!(
        ascii("http://127.1:80 https://[2001:0db8::1]:443 https://[::1]:8443"),
        ["127.0.0.1", "[2001:db8::1]", "[::1]:8443"]
    );
}

#[test]
fn prose_boundaries_and_nested_url_text_match_the_desktop_scanner() {
    assert_eq!(
        ascii(
            "😺 (https://one.test),.!? <http://two.test> \
               \"https://three.test\" 'https://four.test' \
               https://outer.test/https://inner.test"
        ),
        [
            "one.test",
            "two.test",
            "three.test",
            "four.test",
            "outer.test"
        ]
    );
    assert_eq!(
        ascii("https://one.test\u{feff}https://two.test\u{a0}https://three.test"),
        ["one.test", "two.test", "three.test"]
    );
    assert_eq!(
        ascii("https://one.test/path\u{85}https://two.test"),
        ["one.test"]
    );
}

#[test]
fn malformed_and_non_web_tokens_never_supply_a_destination() {
    for text in [
        "",
        "h hh HTTP h😺",
        "https://",
        "https://[:::1]",
        "https://exa%00mple.test",
        "https://example.test:65536",
        "https://xn--example-.org",
        "https://example.org.xn--",
        "mailto:person@example.test",
        "ftp://example.test",
        "httpx://example.test",
    ] {
        assert_eq!(hosts(text), Vec::new(), "{text}");
    }
}
