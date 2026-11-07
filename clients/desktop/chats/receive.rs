//! Apply bounded mailbox pages and authorized pack work under the caller's identity/session lease.
use super::{open, own};
use crate::{profile, search::ProfileCache};
use dyapp_client::{chat::Event, session::Session};
use dyapp_identity::{
    network::{self, Settings},
    Identity,
};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// Mailbox pages one `receive` call reads; the saved cursor lets the next call go on from there.
// ponytail: the next call waits for the poll, schedule it at once if a long backlog feels slow.
const RECEIVE_PAGES: usize = 8;

/// Fetches this device's mailbox on the active network and applies it page by page with
/// `Session::receive_page`, knowing the cached profiles and the published own one, until the
/// mailbox ends, `RECEIVE_PAGES` is reached or `stop` reports an identity transition. An envelope
/// from an owner not cached, such as the Welcome of a first like, gets that owner's profile from
/// the node and is applied on a pass from the head after the last page, as does one from a device
/// the held profile does not list, once a newer profile lists it. One still unknown waits in the
/// mailbox without hiding the pages behind it. Returns the chat-event and completed-pack refresh count.
/// Errors are `notConnected`, [`own::REVOKED`] or a node or storage message; earlier pages stay
/// committed.
pub async fn receive(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    stop: impl Fn() -> bool,
) -> Result<usize, String> {
    crate::profile::finish_link(root, dir, identity, device_key).await?;
    let store = open(dir, identity)?;
    let own = profile::stored(&store)?;
    let own = match own.version {
        0 => None,
        _ => Some(dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?),
    };
    let (epoch, mut cached) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch) {
        return Err("notConnected".into());
    }
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    own::retry_outbox(&mut session, &mut node, &seal_key).await?;
    let mut changed = 0;
    for _ in 0..RECEIVE_PAGES {
        if stop() {
            break;
        }
        let mut from_head = false;
        let mut profiles = cached.all().to_vec();
        profiles.extend(own.clone());
        let page = session
            .receive_page(
                std::slice::from_mut(&mut node),
                device_key,
                &profiles,
                &seal_key,
            )
            .await;
        let (events, more) = match page {
            Ok(page) => page,
            Err(error) => return Err(own::pass_error(&session, &mut node, error).await),
        };
        for event in events {
            match event {
                Event::UnknownOwner(peer) => {
                    if let Ok(verified) = node.get_profile(&peer).await {
                        let newer = cached
                            .get(&peer)
                            .is_none_or(|held| held.profile.version < verified.profile.version);
                        cache
                            .lock()
                            .map_err(|e| e.to_string())?
                            .accept_if_current(epoch, verified.clone());
                        from_head |= cached.accept(verified).is_ok() && newer;
                    }
                }
                Event::Gap(_) | Event::Refused(_) => {}
                _ => changed += 1,
            }
        }
        if !session
            .next_page(more, from_head)
            .map_err(|e| e.to_string())?
        {
            break;
        }
    }
    if !stop() {
        let nodes = std::slice::from_mut(&mut node);
        session
            .respond_sticker_pack_requests(nodes, identity, &seal_key, &stop)
            .await
            .map_err(|error| error.to_string())?;
        // Controls remain hidden; a completed save only asks the UI to refresh local data.
        changed += session
            .download_sticker_packs(nodes, &seal_key, stop)
            .await
            .map_err(|error| error.to_string())?;
    }
    refill_if_due(&mut session, &mut node, &seal_key).await?;
    // Rotates or repeats the package once the first profile publish made one; a failure stays due.
    if matches!(session.store().last_resort(), Ok(Some(_))) {
        let _ = session
            .keep_last_resort(&mut node, device_key, dyapp_client::now(), &seal_key)
            .await;
    }
    // Best effort like the package above; it tries again the next day.
    let now = dyapp_client::now();
    let _ = dyapp_client::publish::refresh_photos(session.store(), &mut node, identity, now).await;
    // A stale copy from a node must not bring back a device the owner dropped.
    if let Ok(newest) = node.get_profile(&identity.peer_id()).await {
        let _ = session.own_device_revoked(&newest.profile);
        if own.is_none_or(|own| own.profile.version <= newest.profile.version) {
            let _ = cached.accept(newest.clone());
            own::refresh_contact(cache, epoch, session.store(), &mut node, &mut cached, now).await;
            let profiles = cached.all().to_vec();
            let nodes = std::slice::from_mut(&mut node);
            let _ = session
                .join_own_devices(nodes, &newest, now, &seal_key)
                .await;
            let _ = session
                .add_own_devices(nodes, &newest, &profiles, now, &seal_key)
                .await;
            let _ = session
                .remove_dropped_devices(nodes, &profiles, now, &seal_key)
                .await;
        }
    }
    Ok(changed)
}

async fn refill_if_due(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    seal_key: &[u8; 32],
) -> Result<(), String> {
    // A catch-up pass runs every minute. A failed refill stays due for the next pass.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    if session
        .store()
        .token_refill_due(now)
        .map_err(|e| e.to_string())?
        && session
            .refill(std::slice::from_mut(node), seal_key)
            .await
            .is_ok()
    {
        session
            .store()
            .token_refill_sent(now)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
