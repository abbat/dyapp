use super::*;
use crate::updates::feed::Platform;
use crate::updates::Version;

const BODY: &[u8] = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.2","tag":"v0.0.2","method":"package"}}}"#;

struct Unreadable;
impl Read for Unreadable {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("status/cache admission must not read this body")
    }
}

#[test]
fn a_valid_body_reaches_only_the_existing_typed_feed_rules() {
    let cache = admit_reply(200, Some("\"feed2\""), BODY, None).unwrap();
    assert_eq!(cache.bytes(), BODY);
    assert_eq!(cache.etag(), Some("\"feed2\""));
    let release = cache.feed().target(Platform::Android).unwrap();
    assert_eq!(
        release.version(),
        Version {
            major: 0,
            minor: 0,
            patch: 2
        }
    );
    assert_eq!(
        release.release_page(),
        "https://github.com/abbat/dyapp/releases/tag/v0.0.2"
    );
    assert!(cache.feed().target(Platform::LinuxX86_64).is_none());
}

#[test]
fn not_modified_requires_cache_and_retains_exact_validated_bytes() {
    assert!(matches!(
        admit_reply(304, None, Unreadable, None),
        Err(Error::NoCache)
    ));
    let cache = Cache::restore(BODY.to_vec(), Some("\"old\"")).unwrap();
    let unchanged = admit_reply(304, None, Unreadable, Some(&cache)).unwrap();
    assert_eq!(unchanged.bytes(), BODY);
    assert_eq!(unchanged.etag(), Some("\"old\""));
    let replaced = admit_reply(304, Some("W/\"new\""), Unreadable, Some(&cache)).unwrap();
    assert_eq!(replaced.bytes(), BODY);
    assert_eq!(replaced.etag(), Some("W/\"new\""));
    assert_eq!(cache.etag(), Some("\"old\""));
}

#[test]
fn redirects_and_other_statuses_never_read_or_replace_the_cache() {
    let cache = Cache::restore(BODY.to_vec(), None).unwrap();
    for status in [301, 302, 303, 307, 308, 400, 404, 429, 500] {
        assert!(
            matches!(admit_reply(status, None, Unreadable, Some(&cache)), Err(Error::Status(code)) if code == status)
        );
    }
    assert_eq!(cache.bytes(), BODY);
}

#[test]
fn corrupt_or_malformed_cache_cannot_authorize_a_conditional_response() {
    for bytes in [
        b"{}".as_slice(),
        b"<html>not a feed</html>",
        b"{\"schema\":2,\"targets\":{}}",
    ] {
        assert!(matches!(
            Cache::restore(bytes.to_vec(), None),
            Err(Error::Feed(feed::Error::Invalid))
        ));
    }
    for etag in [
        "bare",
        "W/bare",
        "\"a b\"",
        "\"a\r\nb\"",
        "\"a\"b\"",
        "\"unterminated",
        "\"\u{00e9}\"",
    ] {
        assert!(
            matches!(Cache::restore(BODY.to_vec(), Some(etag)), Err(Error::Etag)),
            "{etag:?}"
        );
    }
    assert!(Cache::restore(BODY.to_vec(), Some("\"\"")).is_ok());
    assert!(Cache::restore(BODY.to_vec(), Some("W/\"hash\"")).is_ok());
    let oversized = format!("\"{}\"", "x".repeat(feed::MAX_BYTES));
    assert!(matches!(
        Cache::restore(BODY.to_vec(), Some(&oversized)),
        Err(Error::Etag)
    ));
}

#[test]
fn streaming_bound_consumes_only_one_probe_byte_beyond_the_feed_limit() {
    let limit = u64::try_from(feed::MAX_BYTES).unwrap();
    let mut body = std::io::repeat(b' ').take(limit * 2);
    assert!(matches!(
        admit_reply(200, None, &mut body, None),
        Err(Error::Feed(feed::Error::TooLarge))
    ));
    assert_eq!(body.limit(), limit - 1);
    let mut valid_at_limit = BODY.to_vec();
    valid_at_limit.resize(feed::MAX_BYTES, b' ');
    let cache = admit_reply(200, None, valid_at_limit.as_slice(), None).unwrap();
    assert_eq!(cache.bytes().len(), feed::MAX_BYTES);
}

#[test]
fn read_failures_never_become_an_empty_or_partially_valid_feed() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("controlled response failure"))
        }
    }
    assert!(matches!(
        admit_reply(200, None, Broken, None),
        Err(Error::Read(_))
    ));
}
