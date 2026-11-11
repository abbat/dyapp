//! The unpublished edit of the owner's public profile, kept across app restarts.

use super::{Error, Store};
use dyapp_profile::Profile;
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

const KEY: &str = "profile_draft";

impl Store {
    /// Save `draft`, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the encoded draft to the database fails.
    pub fn save_profile_draft(&self, draft: &Profile) -> Result<(), Error> {
        save(&self.db, draft)
    }

    /// Transform the latest draft while retaining SQLite writer ownership through commit.
    ///
    /// Keep `update` short and synchronous: no network, proof search or nested store writes.
    /// Independent connections/processes acquire writer ownership before reading, so they cannot
    /// base an accepted transformation on a draft superseded before its commit.
    ///
    /// # Errors
    ///
    /// Returns admission, decode, transformation or persistence errors. A failed transformation
    /// rolls back without replacing the saved draft.
    pub fn update_profile_draft(
        &mut self,
        update: impl FnOnce(&mut Profile) -> Result<(), Error>,
    ) -> Result<Profile, Error> {
        let transaction = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut draft = load(&transaction)?.unwrap_or_default();
        update(&mut draft)?;
        save(&transaction, &draft)?;
        transaction.commit()?;
        Ok(draft)
    }

    /// The saved draft, or `None` before the first save.
    ///
    /// # Errors
    ///
    /// Returns an error if querying the draft fails, its format is unsupported,
    /// or its saved protobuf cannot be decoded.
    pub fn profile_draft(&self) -> Result<Option<Profile>, Error> {
        load(&self.db)
    }
}

fn save(db: &Connection, draft: &Profile) -> Result<(), Error> {
    db.execute(
        "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
        params![KEY, draft.encode_to_vec()],
    )?;
    Ok(())
}

fn load(db: &Connection) -> Result<Option<Profile>, Error> {
    let value: Option<(i64, Vec<u8>)> = db
        .query_row(
            "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
            [KEY],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((format, value)) = value else {
        return Ok(None);
    };
    if format != 1 {
        return Err(Error::InvalidSetting);
    }
    Profile::decode(value.as_slice())
        .map(Some)
        .map_err(|_| Error::InvalidSetting)
}

#[cfg(test)]
mod tests {
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    #[test]
    fn concurrent_first_opens_share_one_new_database() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        std::thread::scope(|scope| {
            let opens: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        Store::open(&path, &alice)
                            .map(drop)
                            .map_err(|e| e.to_string())
                    })
                })
                .collect();
            for open in opens {
                open.join().unwrap().unwrap();
            }
        });
    }

    #[test]
    fn a_saved_draft_survives_reopening_and_the_newest_wins() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        let store = Store::open(&path, &alice).unwrap();
        assert_eq!(store.profile_draft().unwrap(), None);
        for gender in [
            dyapp_profile::Gender::Woman.code(),
            dyapp_profile::Gender::Man.code(),
        ] {
            let draft = Profile {
                gender,
                birth_month: Some(2),
                ..Profile::default()
            };
            store.save_profile_draft(&draft).unwrap();
        }
        drop(store);
        let draft = Store::open(&path, &alice)
            .unwrap()
            .profile_draft()
            .unwrap()
            .unwrap();
        assert_eq!(
            (draft.gender, draft.birth_month, draft.birth_year),
            (dyapp_profile::Gender::Man.code(), Some(2), None)
        );
    }

    #[test]
    fn transformations_own_the_writer_before_reading_and_preserve_prior_progress() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        let mut first = Store::open(&path, &alice).unwrap();
        let mut second = Store::open(&path, &alice).unwrap();
        // Refusal is immediate and deterministic, not an observation of elapsed time.
        second.db.busy_timeout(std::time::Duration::ZERO).unwrap();
        first
            .update_profile_draft(|draft| {
                let mut entered = false;
                let result = second.update_profile_draft(|_| {
                    entered = true;
                    Ok(())
                });
                assert!(!entered, "another writer entered the transformation");
                assert!(matches!(
                    result,
                    Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _)))
                        if error.code == rusqlite::ErrorCode::DatabaseBusy
                ));
                draft.version = 7;
                draft.pow_bits = 4;
                Ok(())
            })
            .unwrap();
        let saved = second
            .update_profile_draft(|draft| {
                assert_eq!((draft.version, draft.pow_bits), (7, 4));
                draft.gender = dyapp_profile::Gender::Woman.code();
                Ok(())
            })
            .unwrap();
        assert_eq!(first.profile_draft().unwrap(), Some(saved.clone()));
        assert_eq!((saved.version, saved.pow_bits), (7, 4));
        assert_eq!(saved.gender, dyapp_profile::Gender::Woman.code());
    }

    #[test]
    fn a_refused_transformation_rolls_back_and_releases_writer_ownership() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        let mut first = Store::open(&path, &alice).unwrap();
        let mut second = Store::open(&path, &alice).unwrap();
        let original = Profile {
            version: 3,
            ..Profile::default()
        };
        first.save_profile_draft(&original).unwrap();
        let result = first.update_profile_draft(|draft| {
            draft.version = 9;
            Err(Error::InvalidSetting)
        });
        assert!(matches!(result, Err(Error::InvalidSetting)));
        assert_eq!(second.profile_draft().unwrap(), Some(original));
        second.db.busy_timeout(std::time::Duration::ZERO).unwrap();
        second
            .update_profile_draft(|draft| {
                draft.pow_bits = 4;
                Ok(())
            })
            .unwrap();
        let saved = first.profile_draft().unwrap().unwrap();
        assert_eq!((saved.version, saved.pow_bits), (3, 4));
    }

    #[test]
    fn rejects_a_corrupt_draft() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`)
                 VALUES ('profile_draft', 1, x'ff')",
                [],
            )
            .unwrap();
        assert!(matches!(store.profile_draft(), Err(Error::InvalidSetting)));
    }
}
