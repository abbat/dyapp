use super::{Platform, Snapshot, Version};
use crate::updates::availability::{self, Availability, Offer};

impl Snapshot {
    /// Current in-app interruption; dismissing it never changes Settings availability.
    #[must_use]
    pub fn notice(&self, platform: Platform, installed: Version) -> Option<Offer> {
        let Availability::Available(offer) = availability::project(self, platform, installed)
        else {
            return None;
        };
        if self
            .dismissed
            .get(&platform)
            .is_some_and(|seen| *seen >= offer.version())
        {
            return None;
        }
        Some(offer)
    }

    /// Dismiss only the version actually displayed, never a newer concurrently admitted offer.
    /// Persist the changed snapshot before acknowledging the native action.
    pub fn dismiss_notice(
        &mut self,
        platform: Platform,
        installed: Version,
        expected: Version,
    ) -> bool {
        if self
            .notice(platform, installed)
            .is_none_or(|offer| offer.version() != expected)
        {
            return false;
        }
        self.dismissed.insert(platform, expected);
        true
    }

    /// At-most-once OS notification admission; native permission must already be available.
    /// The caller may notify only after committing this snapshot. A crash after commit may
    /// lose the OS alert, but the undismissed in-app notice and Settings badge remain.
    pub fn claim_notification(
        &mut self,
        platform: Platform,
        installed: Version,
        expected: Version,
    ) -> bool {
        if self
            .notice(platform, installed)
            .is_none_or(|offer| offer.version() != expected)
            || self
                .notified
                .get(&platform)
                .is_some_and(|seen| *seen >= expected)
        {
            return false;
        }
        self.notified.insert(platform, expected);
        true
    }

    pub(super) fn notice_marks_valid(&self) -> bool {
        self.dismissed
            .iter()
            .chain(self.notified.iter())
            .all(|(platform, mark)| {
                self.highest(*platform)
                    .is_some_and(|highest| *mark <= highest)
            })
    }

    pub(in crate::updates) fn retains_notice_marks(&self, previous: &Self) -> bool {
        [
            (&previous.dismissed, &self.dismissed),
            (&previous.notified, &self.notified),
        ]
        .into_iter()
        .all(|(old, new)| {
            old.iter()
                .all(|(platform, mark)| new.get(platform).is_some_and(|next| next >= mark))
        })
    }
}

#[cfg(test)]
mod tests;
