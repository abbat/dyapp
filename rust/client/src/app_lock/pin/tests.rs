use super::*;

#[test]
fn existing_format_round_trips_wrong_pin_and_random_salt() {
    let first = hash("123456".into()).unwrap();
    let second = hash("123456".into()).unwrap();
    assert_eq!(first.len(), 49);
    assert_eq!(first[0], 1);
    assert_ne!(first, second);
    assert!(verify("123456".into(), &first).unwrap());
    assert!(!verify("123457".into(), &first).unwrap());
}

#[test]
fn only_six_to_sixty_four_ascii_digits_are_valid_input() {
    assert!(valid_pin("123456"));
    assert!(valid_pin(&"1".repeat(64)));
    for pin in [
        "",
        "12345",
        &"1".repeat(65),
        "12abc6",
        "12345\n",
        " 123456",
        "１２３４５６",
    ] {
        assert!(!valid_pin(pin));
        assert_eq!(hash(pin.into()), Err(Error::InvalidPin));
    }
}

#[test]
fn rejects_malformed_record_before_kdf_work() {
    for record in [vec![], vec![1; 48], vec![1; 50], vec![0; 49], vec![2; 49]] {
        assert_eq!(verify("123456".into(), &record), Err(Error::InvalidRecord));
    }
    assert_eq!(verify("12abc6".into(), &[]), Err(Error::InvalidPin));
    assert_eq!(
        Error::InvalidPin.to_string(),
        "PIN must contain 6 to 64 decimal digits"
    );
    assert_eq!(
        Error::InvalidRecord.to_string(),
        "Invalid PIN verifier record"
    );
    assert_eq!(Error::Unavailable.to_string(), "PIN verifier unavailable");
}
