//! Shared explicit actions; retry/stop never open MLS or require a published device.
use super::{status, Error, State};
use crate::{
    session::Session,
    sticker_media,
    store::{messages::MessageKey, sticker_packs::exchange::download::PackDownloadJob, Store},
};

/// Explicit received-pack operation.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// Request only when the current status is New. A repeated click is a no-op.
    Request,
    /// Retry the exact current descriptor without sending a new control.
    Retry,
    /// Replace the exact current request generation with a fresh queued control.
    Fresh,
    /// Retire acceptance for the exact generation, without recalling queued packets.
    StopWaiting,
}

/// Operation and the generation displayed to the caller; core validates their combination.
#[derive(Clone, Copy, Debug)]
pub struct Action {
    /// Explicit operation.
    pub kind: Kind,
    /// Absent for Request, required for every other operation.
    pub generation: Option<[u8; 16]>,
}

/// Admitted staging context. Private fields prevent adapters from inventing a selection or job.
pub struct Stage {
    owner: String,
    peer: String,
    key: MessageKey,
    job: Option<PackDownloadJob>,
}

fn current_job(
    store: &Store,
    peer: &str,
    pack_id: [u8; 32],
    expected: Option<[u8; 16]>,
) -> Result<PackDownloadJob, Error> {
    let pending = store
        .pending_sticker_pack(peer, &pack_id)?
        .ok_or(Error::RequestChanged)?;
    if expected != Some(pending.token) {
        return Err(Error::RequestChanged);
    }
    Ok(PackDownloadJob {
        peer: peer.into(),
        pack_id,
        sticker_hash: pending.sticker_hash,
        token: pending.token,
    })
}

fn published(store: &Store) -> Result<dyapp_profile::Profile, Error> {
    let own = store.profile_draft()?.unwrap_or_default();
    if own.version == 0 {
        return Err(Error::Unpublished);
    }
    Ok(own)
}

/// Apply a local action or prepare an offline-first control. None means local completion/no-op.
/// Keep the adapter's identity/session guard across preparation and any returned staging.
/// No MLS session is needed for Retry, Stop waiting or a repeated initial Request.
///
/// # Errors
/// Refuses ineligible selections, stale generations, storage failure and unpublished staging.
pub fn prepare(
    store: &mut Store,
    seal: &[u8; 32],
    peer: &str,
    key: &MessageKey,
    action: Action,
) -> Result<Option<Stage>, Error> {
    let item = sticker_media::select(store, peer, key)?.message().clone();
    if item.key.author != peer {
        return Err(sticker_media::Error::UnknownMessage.into());
    }
    let job = match action.kind {
        Kind::Request => {
            if action.generation.is_some() {
                return Err(Error::RequestChanged);
            }
            if status(store, seal, peer, key)?.state != State::New {
                return Ok(None);
            }
            None
        }
        _ => Some(current_job(
            store,
            peer,
            item.sticker.pack_id,
            action.generation,
        )?),
    };
    if finish_local(store, action.kind, job.as_ref())? {
        return Ok(None);
    }
    published(store)?;
    Ok(Some(Stage {
        owner: store.owner_peer_id().into(),
        peer: peer.into(),
        key: key.clone(),
        job,
    }))
}

fn finish_local(store: &Store, kind: Kind, job: Option<&PackDownloadJob>) -> Result<bool, Error> {
    match (kind, job) {
        (Kind::Retry, Some(job)) => {
            store
                .retry_sticker_pack_download(job)?
                .ok_or(Error::RequestChanged)?;
        }
        (Kind::StopWaiting, Some(job)) => {
            if !store.finish_sticker_pack(&job.peer, &job.pack_id, &job.token)? {
                return Err(Error::RequestChanged);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

impl Stage {
    /// Queue control, intent and exact MLS writes through the existing session transaction.
    /// This is local commitment, not node acceptance or delivery. No network is contacted.
    ///
    /// # Errors
    /// Refuses a different owner, changed selection/generation, unpublished or unlisted device,
    /// or session/staging failure. A failed MLS commit can invalidate the in-memory session.
    pub fn stage(self, session: &mut Session, seal: &[u8; 32]) -> Result<(), Error> {
        if session.store().owner_peer_id() != self.owner {
            return Err(sticker_media::Error::IdentityChanged.into());
        }
        // Revalidate after the adapter opens/reuses MLS; a prepared plan is not authority.
        sticker_media::select(session.store(), &self.peer, &self.key)?;
        let own = published(session.store())?;
        if !own.devices.contains(&session.certificate()?) {
            return Err(Error::Unpublished);
        }
        match self.job {
            Some(job) => {
                session.stage_fresh_sticker_pack_request(&job, seal)?;
            }
            None => {
                session.stage_sticker_pack_request(&self.peer, &self.key, seal)?;
            }
        }
        Ok(())
    }
}
