//! Native transport of the shared received-view projection; no network or calendar policy here.
use super::{failed, ChatSession};
use crate::CoreError;

/// One latest locally received UTC viewing date, not an exact time or an unread indication.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReceivedProfileView {
    /// Original canonical sender and action target.
    pub peer_id: String,
    /// Core-formatted `YYYY-MM-DD` UTC date.
    pub utc_date: String,
}

impl From<dyapp_client::store::ReceivedProfileView> for ReceivedProfileView {
    fn from(value: dyapp_client::store::ReceivedProfileView) -> Self {
        Self {
            peer_id: value.peer_id,
            utc_date: value.utc_date,
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Read bounded latest received dates in the core's moderation and newest-first order.
    /// Receiving is independent of the send preference; this never fetches profiles or sends views.
    ///
    /// # Errors
    /// Returns session-lock, storage, result-bound or invalid stored-date errors.
    pub fn received_profile_views(&self) -> Result<Vec<ReceivedProfileView>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session
            .store()
            .received_profile_views()
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(failed)
    }
}
