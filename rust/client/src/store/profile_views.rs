//! Read-only received profile-view history; the protocol records a UTC day, not a viewing time.
use super::{collect_bounded, Error, Store, MAX_RESULT_ROWS};
use dyapp_profile::CivilDate;

/// Latest locally received viewing day for one other profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedProfileView {
    /// Canonical sender identity; this remains the profile-opening action target.
    pub peer_id: String,
    /// Available date only, as `YYYY-MM-DD` in UTC; never an invented exact timestamp.
    pub utc_date: String,
}

impl Store {
    /// Latest received views, newest UTC day first with Peer ID order for ties.
    ///
    /// Sending preferences do not gate this read. Blocked, passed and closed-conversation peers
    /// stay hidden under the same moderation boundary as Liked you; matches remain visible.
    /// Returns at most the common store result bound, without fetching profiles or emitting events.
    ///
    /// # Errors
    /// Returns query/decoding, invalid calendar-day or result-bound errors instead of a partial list.
    pub fn received_profile_views(&self) -> Result<Vec<ReceivedProfileView>, Error> {
        let mut query = self.db.prepare(
            "SELECT `views`.`sender`, MAX(`views`.`day`) AS `latest`
             FROM `view_days` AS `views`
             WHERE `views`.`target` = ?1 AND `views`.`sender` <> ?1
             AND NOT EXISTS (SELECT 1 FROM `peers` WHERE `peer_id` = `views`.`sender`
                 AND `blocked` = 1)
             AND NOT EXISTS (SELECT 1 FROM `pass_history` WHERE `peer_id` = `views`.`sender`
                 AND `passed` = 1)
             AND NOT EXISTS (SELECT 1 FROM `conversations`
                 WHERE `peer_id` = `views`.`sender` AND `state` = 4)
             GROUP BY `views`.`sender`
             ORDER BY `latest` DESC, `views`.`sender`",
        )?;
        let rows = query.query_map([&self.owner_peer_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        collect_bounded(rows, MAX_RESULT_ROWS)?
            .into_iter()
            .map(|(peer_id, day)| {
                let day = u64::try_from(day).map_err(|_| Error::ClockRange)?;
                let date = CivilDate::from_unix_days(day)
                    .ok_or(Error::ClockRange)?
                    .yyyymmdd();
                Ok(ReceivedProfileView {
                    peer_id,
                    utc_date: format!(
                        "{:04}-{:02}-{:02}",
                        date / 10_000,
                        date / 100 % 100,
                        date % 100
                    ),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
