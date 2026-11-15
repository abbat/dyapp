use super::*;

#[test]
fn assistance_keeps_only_ascii_digits_and_caps_at_the_existing_boundary() {
    for (entered, expected) in [
        (String::new(), String::new()),
        ("a1-2 3\n4.5/6".to_owned(), "123456".to_owned()),
        ("１２٣a1-23𐐷456٧".to_owned(), "123456".to_owned()),
        ("1".repeat(64), "1".repeat(64)),
        ("2".repeat(65), "2".repeat(64)),
    ] {
        assert_eq!(normalize_input(entered), expected);
    }
}

#[test]
fn confirmation_prioritizes_validity_and_does_not_authorize_an_unlock() {
    for (pin, confirmed, expected) in [
        (
            "12345".to_owned(),
            "12345".to_owned(),
            Confirmation::Incomplete,
        ),
        (
            "12345".to_owned(),
            "other".to_owned(),
            Confirmation::Incomplete,
        ),
        (
            "12abc6".to_owned(),
            "12abc6".to_owned(),
            Confirmation::Incomplete,
        ),
        (
            "123456".to_owned(),
            "123457".to_owned(),
            Confirmation::Mismatch,
        ),
        (
            "123456".to_owned(),
            "123456".to_owned(),
            Confirmation::Ready,
        ),
        ("1".repeat(64), "1".repeat(64), Confirmation::Ready),
        ("1".repeat(65), "1".repeat(65), Confirmation::Incomplete),
    ] {
        assert_eq!(confirmation(&pin, &confirmed), expected);
    }
}

#[test]
fn assistance_does_not_make_authentication_normalize_invalid_raw_input() {
    let entered = "1a23456";
    assert_eq!(normalize_input(entered.into()), "123456");
    assert!(!valid_pin(entered));
    assert_eq!(hash(entered.into()), Err(Error::InvalidPin));
    assert_eq!(verify(entered.into(), &[]), Err(Error::InvalidPin));
}
