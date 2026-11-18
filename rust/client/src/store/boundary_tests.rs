use super::tests::temp_db_path;
use super::{collect_bounded, Error, Store, MAX_VALUE_BYTES, VERSION};
use dyapp_identity::Identity;
use rusqlite::Connection;
use std::path::Path;

fn write_limit_probe(path: &Path, value_bytes: usize) {
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE `limit_probe` (`id` INTEGER PRIMARY KEY, `value` BLOB)")
        .unwrap();
    for (id, value) in [(1, value_bytes), (3, value_bytes + 1)] {
        db.execute(
            "INSERT INTO `limit_probe` (`id`, `value`) VALUES (?1, ?2)",
            rusqlite::params![id, vec![7_u8; value]],
        )
        .unwrap();
    }
    for (id, value) in [(2, value_bytes), (4, value_bytes + 1)] {
        db.execute(
            "INSERT INTO `limit_probe` (`id`, `value`) VALUES (?1, ?2)",
            rusqlite::params![id, "x".repeat(value)],
        )
        .unwrap();
    }
}

#[test]
fn sqlite_values_accept_the_limit_and_reject_one_byte_more() {
    let path = temp_db_path();
    let identity = Identity::try_generate().unwrap();
    drop(Store::open(&path, &identity).unwrap());
    let value_bytes = usize::try_from(MAX_VALUE_BYTES).unwrap();
    write_limit_probe(&path, value_bytes);
    let store = Store::open(&path, &identity).unwrap();
    let blob: Vec<u8> = store
        .db
        .query_row(
            "SELECT `value` FROM `limit_probe` WHERE `id` = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let text: String = store
        .db
        .query_row(
            "SELECT `value` FROM `limit_probe` WHERE `id` = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(blob.len(), value_bytes);
    assert_eq!(text.len(), value_bytes);
    assert!(store
        .db
        .query_row(
            "SELECT `value` FROM `limit_probe` WHERE `id` = 3",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .is_err());
    assert!(store
        .db
        .query_row(
            "SELECT `value` FROM `limit_probe` WHERE `id` = 4",
            [],
            |row| row.get::<_, String>(0),
        )
        .is_err());
}

#[test]
fn bounded_collection_accepts_the_limit_and_rejects_one_row_more() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE `entries` (`value` INTEGER)")
        .unwrap();
    for value in 0..3 {
        db.execute("INSERT INTO `entries` (`value`) VALUES (?1)", [value])
            .unwrap();
    }
    let mut query = db
        .prepare("SELECT `value` FROM `entries` ORDER BY `value`")
        .unwrap();
    assert!(matches!(
        collect_bounded(query.query_map([], |row| row.get::<_, i64>(0)).unwrap(), 2),
        Err(Error::ResultTooLarge(2))
    ));
    let values = collect_bounded(
        query
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .take(2),
        2,
    )
    .unwrap();
    assert_eq!(values, [0, 1]);
}

#[test]
fn binds_database_to_one_identity_across_reopens() {
    let path = temp_db_path();
    let alice = Identity::try_generate().unwrap();
    let bob = Identity::try_generate().unwrap();
    let store = Store::open(&path, &alice).unwrap();
    drop(store);
    assert!(matches!(
        Store::open(&path, &bob),
        Err(Error::WrongIdentity)
    ));
    drop(Store::open(&path, &alice).unwrap());
}

#[test]
fn refuses_future_and_unknown_schemas_without_replacing_them() {
    let path = temp_db_path();
    let alice = Identity::try_generate().unwrap();
    drop(Store::open(&path, &alice).unwrap());
    let db = Connection::open(&path).unwrap();
    db.pragma_update(None, "user_version", VERSION + 1).unwrap();
    drop(db);
    assert!(matches!(
        Store::open(&path, &alice),
        Err(Error::FutureVersion(v)) if v == VERSION + 1
    ));

    let unknown = temp_db_path();
    let db = Connection::open(&unknown).unwrap();
    db.execute_batch("CREATE TABLE `unknown` (`value` INTEGER)")
        .unwrap();
    drop(db);
    assert!(matches!(
        Store::open(&unknown, &alice),
        Err(Error::Unversioned)
    ));
}

#[test]
fn refuses_relative_path_and_corrupt_database() {
    let alice = Identity::try_generate().unwrap();
    assert!(matches!(
        Store::open(std::path::Path::new("client.db"), &alice),
        Err(Error::RelativePath)
    ));
    let path = temp_db_path();
    std::fs::write(&path, b"not a SQLite database").unwrap();
    assert!(matches!(Store::open(&path, &alice), Err(Error::Sql(_))));
}
