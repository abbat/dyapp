use super::*;

#[test]
fn decimal_mib_preserves_exact_bytes_and_rejects_rounding_or_overflow() {
    for (text, bytes) in [
        ("1", 1_048_576),
        ("1.5", 1_572_864),
        ("+1.5e2", 157_286_400),
        ("15E-1", 1_572_864),
        ("10240", 10_737_418_240),
        ("  \u{feff}1.00000095367431640625\n", 1_048_577),
    ] {
        assert_eq!(parse_mib(text), Some(bytes), "{text:?}");
    }
    for text in [
        "",
        ".",
        "-1",
        "0",
        "0.999999",
        "10240.00000095367431640625",
        "1.0000001",
        "NaN",
        "inf",
        "1e309",
        "1e-309",
        "1e",
        "1e1e1",
        "1.2.3",
        "0x10",
        "1_024",
        "1 024",
        "18446744073709551615",
    ] {
        assert_eq!(parse_mib(text), None, "{text:?}");
    }
}

#[test]
fn decimal_display_round_trips_non_whole_mib_budgets_without_float_loss() {
    for bytes in [
        MediaCacheConfig::MIN_LIMIT_BYTES,
        MediaCacheConfig::MIN_LIMIT_BYTES + 1,
        123_456_789,
        393_216_123,
        MediaCacheConfig::MAX_LIMIT_BYTES - 1,
        MediaCacheConfig::MAX_LIMIT_BYTES,
    ] {
        assert_eq!(parse_mib(&mib_text(bytes)), Some(bytes));
    }
    assert_eq!(mib_text(1_048_576), "1");
    assert_eq!(mib_text(1_572_864), "1.5");
    assert_eq!(mib_text(1_048_577), "1.00000095367431640625");
}

#[test]
fn destructive_confirmation_matches_retention_budget_and_actual_usage() {
    let current = MediaCacheConfig {
        enabled: true,
        limit_bytes: 8_388_608,
    };
    let smaller = MediaCacheConfig {
        limit_bytes: 4_194_304,
        ..current
    };
    let larger = MediaCacheConfig {
        limit_bytes: 16_777_216,
        ..current
    };
    let off = MediaCacheConfig {
        enabled: false,
        ..current
    };
    assert!(requires_confirmation(current, smaller, 0));
    assert!(requires_confirmation(current, off, 0));
    assert!(!requires_confirmation(current, larger, 0));
    assert!(!requires_confirmation(current, current, 0));
    assert!(requires_confirmation(current, larger, 16_777_217));
    assert!(!requires_confirmation(current, larger, 16_777_216));
    assert!(!requires_confirmation(off, off, 0));
    assert!(requires_confirmation(off, off, 1));
    assert!(!requires_confirmation(off, current, 0));
    assert!(!requires_confirmation(off, smaller, 0));
    assert!(requires_confirmation(off, smaller, 4_194_305));
}
