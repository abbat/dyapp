//! Explicit native actions delegate eligibility, generation and staging to the common core.
use super::{failed, ChatSession, StickerPackSaveStatus};
use crate::CoreError;
use dyapp_client::{
    sticker_requests as core,
    store::{messages::MessageKey, Store},
};

/// Explicit operation on a selected received pack. Tokens are local intent generations.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum StickerPackSaveAction {
    /// Initial Save; core makes repeated clicks a no-op.
    Request,
    /// Retry a retained descriptor; core rotates its generation without sending a control.
    Retry {
        /// Exact 16-byte generation shown by current status.
        generation: Vec<u8>,
    },
    /// Queue a fresh descriptor request, replacing the current generation.
    Fresh {
        /// Exact 16-byte generation shown by current status.
        generation: Vec<u8>,
    },
    /// Retire local acceptance; already queued packets are not recalled.
    StopWaiting {
        /// Exact 16-byte generation shown by current status.
        generation: Vec<u8>,
    },
}
impl TryFrom<StickerPackSaveAction> for core::Action {
    type Error = CoreError;

    fn try_from(value: StickerPackSaveAction) -> Result<Self, Self::Error> {
        let (kind, generation) = match value {
            StickerPackSaveAction::Request => (core::Kind::Request, None),
            StickerPackSaveAction::Retry { generation } => (core::Kind::Retry, Some(generation)),
            StickerPackSaveAction::Fresh { generation } => (core::Kind::Fresh, Some(generation)),
            StickerPackSaveAction::StopWaiting { generation } => {
                (core::Kind::StopWaiting, Some(generation))
            }
        };
        Ok(Self {
            kind,
            generation: generation
                .map(|bytes| bytes.try_into().map_err(|_| failed("requestChanged")))
                .transpose()?,
        })
    }
}

/// The core action completed locally, including an idempotent no-op, without a delivery claim.
/// A later read failure must never make the caller blindly repeat an already applied action.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StickerPackSaveAttempt {
    /// Authoritative status after applying the action.
    Ready {
        /// Core-computed status, including a rotated generation after Retry/Fresh.
        status: StickerPackSaveStatus,
    },
    /// Application completed, but current status could not be read; refresh instead of resubmitting.
    Unavailable {
        /// Local projection failure.
        detail: String,
    },
}
impl StickerPackSaveAttempt {
    pub(super) fn from_status(status: Result<core::Status, core::Error>) -> Self {
        match status {
            Ok(status) => Self::Ready {
                status: status.into(),
            },
            Err(error) => Self::Unavailable {
                detail: error.to_string(),
            },
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Apply a received-pack action offline, using the admitted session and core durable outbox.
    /// Call off the UI thread. Ownership remains held through application and status readback.
    /// Success means local application/no-op only, never node acceptance or peer delivery.
    ///
    /// # Errors
    /// Returns malformed selection/generation, eligibility, locking, storage, publication/device
    /// listing or staging failures. A staging failure may invalidate the in-memory MLS session.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn change_sticker_pack_request(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
        action: StickerPackSaveAction,
    ) -> Result<StickerPackSaveAttempt, CoreError> {
        let key = MessageKey {
            group: group_id,
            author,
            id: id.try_into().map_err(|_| failed("unknownMessage"))?,
        };
        let action = action.try_into()?;
        let mut session = self.session.lock().map_err(failed)?;
        let mut store =
            Store::open(&self.dir.join("client.db"), &self.key.identity).map_err(failed)?;
        if let Some(stage) =
            core::prepare(&mut store, &self.seal_key, &peer_id, &key, action).map_err(failed)?
        {
            stage.stage(&mut session, &self.seal_key).map_err(failed)?;
        }
        Ok(StickerPackSaveAttempt::from_status(core::status(
            &mut store,
            &self.seal_key,
            &peer_id,
            &key,
        )))
    }
}
