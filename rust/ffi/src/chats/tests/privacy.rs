//! Public native adapters retain common defaults, errors and independent per-identity fields.
use super::favorites::Fixture;
use super::*;

fn values(chats: &ChatSession) -> (bool, bool, bool, bool, ChatNotice) {
    (
        chats.profile_views_enabled().unwrap(),
        chats.typing_enabled().unwrap(),
        chats.read_receipts().unwrap(),
        chats.presence_enabled().unwrap(),
        chats.notice().unwrap(),
    )
}

fn database(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open(std::path::Path::new(&f.dir).join("client.db")).unwrap()
}

#[test]
fn native_privacy_preferences_are_independent_persistent_and_identity_bound() {
    let f = Fixture::new();
    assert_eq!(
        values(&f.chats),
        (false, true, false, true, ChatNotice::Fixed)
    );
    f.chats.set_profile_views_enabled(true).unwrap();
    assert_eq!(
        values(&f.chats),
        (true, true, false, true, ChatNotice::Fixed)
    );
    f.chats.set_typing_enabled(false).unwrap();
    assert_eq!(
        values(&f.chats),
        (true, false, false, true, ChatNotice::Fixed)
    );
    f.chats.set_read_receipts(true).unwrap();
    f.chats.set_presence_enabled(false).unwrap();
    f.chats.set_notice(ChatNotice::Name).unwrap();
    f.chats.set_profile_views_enabled(false).unwrap();
    assert_eq!(
        values(&f.chats),
        (false, false, true, false, ChatNotice::Name)
    );
    f.chats.set_typing_enabled(true).unwrap();
    assert_eq!(
        values(&f.chats),
        (false, true, true, false, ChatNotice::Name)
    );
    let reopened = f.reopen();
    assert_eq!(
        values(&reopened.chats),
        (false, true, true, false, ChatNotice::Name)
    );
    assert_eq!(
        values(&Fixture::new().chats),
        (false, true, false, true, ChatNotice::Fixed)
    );
}

#[test]
fn native_privacy_reads_refuse_unknown_format_or_invalid_flag_without_repair() {
    let f = Fixture::new();
    let db = database(&f);
    for (key, read) in [
        (
            "send_profile_views",
            ChatSession::profile_views_enabled
                as fn(&ChatSession) -> Result<bool, crate::CoreError>,
        ),
        (
            "hide_typing",
            ChatSession::typing_enabled as fn(&ChatSession) -> Result<bool, crate::CoreError>,
        ),
    ] {
        for (format, bytes) in [(2_i64, vec![0u8]), (1, vec![2]), (1, vec![0, 1])] {
            db.execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)
                 ON CONFLICT (`key`) DO UPDATE SET `format_version` = excluded.`format_version`,
                     `value` = excluded.`value`",
                rusqlite::params![key, format, &bytes],
            )
            .unwrap();
            assert_eq!(
                read(&f.chats).unwrap_err().to_string(),
                "client store setting is invalid"
            );
            let stored: (i64, Vec<u8>) = db
                .query_row(
                    "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(stored, (format, bytes));
        }
    }
}

#[test]
fn native_privacy_write_failure_does_not_claim_success_or_change_other_fields() {
    let f = Fixture::new();
    let before = values(&f.chats);
    database(&f)
        .execute_batch(
            "CREATE TRIGGER `refuse_fixture_settings` BEFORE INSERT ON `settings`
             BEGIN SELECT RAISE(FAIL, 'fixture write refusal'); END;",
        )
        .unwrap();
    assert!(f.chats.set_profile_views_enabled(true).is_err());
    assert!(f.chats.set_typing_enabled(false).is_err());
    assert_eq!(values(&f.chats), before);
}
