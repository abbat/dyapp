#![expect(unreachable_pub, reason = "Generated Tauri wrapper reexports")]
//! The new computer's side of device linking: the code, the wait for the record and the slot the
//! record becomes. The device key and the one-use key stay in memory until the record arrives, so
//! nothing of a link survives a restart before that. [`crate::profile::finish_link`] does the rest.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use dyapp_client::identity_slots::{self as core, Layout, Registry};
use dyapp_client::link::{Invite, Linked};
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::safety::QrMatrix;
use crate::slots::{self, Opened, Slot};
use crate::{slot_keys, IdentityState};

struct Pending {
    invite: Invite,
    device: Identity,
    dir: PathBuf,
}

#[derive(Default)]
struct Link {
    pending: Option<Pending>,
    /// Bumped by a start or a cancel, so a poll that was running then finds its link gone.
    turn: u64,
    /// The slot this run's wizard made: a link replaces it, as it was made only to reach a node.
    wizard: Option<String>,
}

/// The link this computer waits for, at most one.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Links(Mutex<Link>);

impl Links {
    fn lock(&self) -> MutexGuard<'_, Link> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Notes slot `id` as made by this run.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn made(&self, id: &str) {
        self.lock().wizard = Some(id.to_owned());
    }

    fn take(&self, dir: &Path) -> Option<(Pending, u64)> {
        let mut link = self.lock();
        let pending = link.pending.take().filter(|pending| pending.dir == dir)?;
        Some((pending, link.turn))
    }
}

/// A code to show, as text and as a QR matrix.
#[derive(serde::Serialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Shown {
    code: String,
    qr: QrMatrix,
}

/// Starts a link from the open identity, in place of any earlier one.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn start_link(
    state: tauri::State<'_, IdentityState>,
    links: tauri::State<'_, Links>,
) -> Result<Shown, String> {
    let lease = state.lease()?;
    let device = Identity::generate();
    let mailbox = dyapp_client::mailbox(&device.public_key());
    let invite = Invite::new(mailbox, dyapp_client::now()).map_err(|e| e.to_string())?;
    let code = invite.code().text();
    let qr = crate::safety::matrix(code.as_bytes())?;
    let dir = lease.active().1.clone();
    let mut link = links.lock();
    link.turn += 1;
    link.pending = Some(Pending {
        invite,
        device,
        dir,
    });
    Ok(Shown { code, qr })
}

/// Forgets the link. A record that arrives later cannot be opened.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn cancel_link(links: tauri::State<'_, Links>) {
    let mut link = links.lock();
    link.turn += 1;
    link.pending = None;
}

/// Whether the open identity is linked but has not listed itself yet.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn link_pending(state: tauri::State<'_, IdentityState>) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = crate::profile::open(dir, identity)?;
    let pending = store.link_pending().map_err(|e| e.to_string())?;
    Ok(pending.is_some())
}

#[cfg(feature = "ui-smoke")]
/// UI test only: hands the code to the smoke, which types it into the other client.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn ui_test_link_code(app: tauri::AppHandle, code: String) -> Result<(), String> {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return Err("UI test only".into());
    }
    std::fs::write(crate::config_dir(&app)?.join("link-code"), code).map_err(|e| e.to_string())
}

/// Whether the open identity came to this computer by a link.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn linked_device(state: tauri::State<'_, IdentityState>) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = crate::profile::open(dir, identity)?;
    store.is_linked_device().map_err(|e| e.to_string())
}

async fn receive(pending: &mut Pending, now: u64) -> Result<Option<Linked>, String> {
    let active = network::load(&pending.dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = crate::profile::connect(&active).await?;
    pending
        .invite
        .receive(&mut node, &pending.device, now)
        .await
        .map_err(|e| e.to_string())
}

/// Looks for the record once. `None` means keep waiting; a record opens its new slot, with the
/// result of `identity_state`. Errors are `expired`, which ends the link, or `notConnected` or a
/// node message, which do not: the next poll tries again. A link started elsewhere or left behind
/// by a change of identity is forgotten here.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
pub async fn poll_link(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    links: tauri::State<'_, Links>,
) -> Result<Option<HashMap<&'static str, String>>, String> {
    let root = crate::config_dir(&app)?;
    let lease = state.lease()?;
    let (_, dir) = lease.active().clone();
    let Some((mut pending, turn)) = links.take(&dir) else {
        return Ok(None);
    };
    let now = dyapp_client::now();
    if now >= pending.invite.code().expires_at {
        return Err("expired".into());
    }
    // The client's node is not `Sync`, as in `crate::publish_profile`.
    let (pending, received) = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let received = tauri::async_runtime::block_on(receive(&mut pending, now));
        (pending, received)
    })
    .await
    .map_err(|e| e.to_string())?;
    let (linked, wizard) = {
        let mut link = links.lock();
        if link.turn != turn {
            return Ok(None);
        }
        match received {
            Ok(Some(linked)) => (linked, link.wizard.clone()),
            other => {
                link.pending = Some(pending);
                return other.map(|_| None);
            }
        }
    };
    let transition = crate::drain_identity(&app, &state).await?;
    let recover_dir = root.clone();
    let written = tauri::async_runtime::spawn_blocking(move || {
        write(&root, &pending, &linked, wizard.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|written| written);
    match written {
        Ok((slot, identity)) => crate::respond(Opened::Ready(slot, identity), transition).map(Some),
        Err(error) => {
            IdentityState::reconcile(recover_dir, transition).await?;
            Err(error)
        }
    }
}

/// Makes the slot of `linked` in the storage mode of the slot the wait ran in, through the core
/// registry. It removes that slot when it is `wizard`'s and holds no draft: the identity made this
/// run may have been published since, and the link then came from Settings.
fn write(
    root: &Path,
    pending: &Pending,
    linked: &Linked,
    wizard: Option<&str>,
) -> Result<(Slot, Identity), String> {
    let from = slots::list(root)?
        .into_iter()
        .find(|slot| slot.dir == pending.dir)
        .ok_or("Unknown identity")?
        .id;
    let device = |slot: &core::Slot| {
        let slot = slot_keys::local(slot);
        let (path, name) = (slot.device_path(), slot.device_name());
        crate::save_key(&slot, &path, &name, &pending.device)?;
        match crate::load_key(&slot, &path, &name)? {
            Some(saved) if saved.public_key() == pending.device.public_key() => Ok(()),
            _ => Err("Saved device key does not reopen".into()),
        }
    };
    let registry = Registry::new(root.to_path_buf(), Layout::Desktop);
    let wizard = wizard == Some(from.as_str());
    let (slot, identity, _) = registry.link(&from, linked, wizard, &slot_keys::Keys, &device)?;
    Ok((slot_keys::local(&slot), identity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::network::Network;
    use dyapp_profile::Profile;
    use std::fs;

    fn run(name: &str, wizard: bool, published: bool) -> bool {
        let root = std::env::temp_dir().join(format!(
            "dyapp-link-{}-{name}-{}",
            std::process::id(),
            dyapp_client::now()
        ));
        fs::create_dir_all(&root).unwrap();
        let (old, current) = slots::create(&root, "file", &crate::save, &crate::load).unwrap();
        let rows = Network {
            seeds: vec!["127.0.0.1:1".into()],
            name: String::new(),
        };
        network::save(&old.dir, Settings::Active, &rows).unwrap();
        if published {
            let store = crate::profile::open(&old.dir, &current).unwrap();
            store.save_profile_draft(&Profile::default()).unwrap();
        }
        let device = Identity::generate();
        let pending = Pending {
            invite: Invite::new([0; 32], dyapp_client::now()).unwrap(),
            device: Identity::from_secret(&device.secret()),
            dir: old.dir.clone(),
        };
        let linked = Linked {
            identity: Identity::generate(),
            state: b"state".to_vec(),
        };
        let wizard = wizard.then_some(old.id.as_str());
        let (slot, _) = write(&root, &pending, &linked, wizard).unwrap();

        assert_eq!(slots::active(&root).as_deref(), Some(slot.id.as_str()));
        let opened = crate::load(&slot).unwrap().unwrap();
        assert_eq!(opened.peer_id(), linked.identity.peer_id());
        let saved = crate::device_key(&root, &slot.dir, &linked.identity).unwrap();
        assert_eq!(saved.public_key(), device.public_key());
        let copied = network::load(&slot.dir, Settings::Active).unwrap();
        assert_eq!(copied, Some(rows));
        let store = crate::profile::open(&slot.dir, &linked.identity).unwrap();
        assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
        // Windows cannot remove a directory with an open database in it.
        drop(store);

        let kept = slots::list(&root).unwrap().iter().any(|s| s.id == old.id);
        fs::remove_dir_all(root).unwrap();
        kept
    }

    #[test]
    fn a_record_makes_a_ready_slot_and_only_an_empty_wizard_slot_goes() {
        assert!(!run("wizard", true, false));
        assert!(run("settings", false, false));
        assert!(run("published", true, true));
    }
}
