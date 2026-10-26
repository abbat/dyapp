//! The unpublished edit of the owner's public profile, kept across app restarts.

use super::{Error, Store};
use dyapp_profile::Profile;
use prost::Message;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "profile_draft";

impl Store {
    /// Save `draft`, replacing any earlier one.
    pub fn save_profile_draft(&self, draft: &Profile) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, draft.encode_to_vec()],
        )?;
        Ok(())
    }

    /// The saved draft, or `None` before the first save.
    pub fn profile_draft(&self) -> Result<Option<Profile>, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
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
}

#[cfg(test)]
mod tests {
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    #[test]
    fn a_saved_draft_survives_reopening_and_the_newest_wins() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        let store = Store::open(&path, &alice).unwrap();
        assert_eq!(store.profile_draft().unwrap(), None);
        for gender in ["woman", "man"] {
            let draft = Profile {
                gender: gender.into(),
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
            (draft.gender.as_str(), draft.birth_month, draft.birth_year),
            ("man", Some(2), None)
        );
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
