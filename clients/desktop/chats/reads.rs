//! Opt-in read receipts of one open chat; see docs/design/read-receipts.md.

use std::collections::BTreeMap;
use std::path::Path;

use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use super::MessageView;
use crate::profile;

type PendingRead = (Vec<u8>, [u8; 16]);

/// The latest partner text of each group that still owes a receipt. A request owes none: only an
/// accepted conversation may learn when the chat was opened.
fn due_reads(store: &Store, peer_id: &str) -> Result<Vec<PendingRead>, String> {
    if store.conversation(peer_id).map_err(|e| e.to_string())? != Conversation::Open {
        return Ok(Vec::new());
    }
    let messages = store
        .current_chat_messages(peer_id)
        .map_err(|e| e.to_string())?;
    let latest: BTreeMap<_, _> = messages
        .into_iter()
        .filter(|message| message.key.author == peer_id)
        .map(|message| (message.key.group, message.key.id))
        .collect();
    let mut due = Vec::new();
    for (group, id) in latest {
        if store
            .draft_read(&group, peer_id, id)
            .map_err(|e| e.to_string())?
        {
            due.push((group, id));
        }
    }
    Ok(due)
}

/// Send what the shown chat owes and return how many receipts went out. With nothing due, which
/// is always the case while the setting is off, this never touches the network.
pub(super) async fn send_read_receipts(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
) -> Result<usize, String> {
    let store = super::open_visible_chat(dir, identity, peer_id)?;
    let due = due_reads(&store, peer_id)?;
    if due.is_empty() {
        return Ok(0);
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create_with_scores(
        store,
        identity,
        device_key,
        &seal_key,
        dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for (group, seen) in &due {
        session
            .mark_read(std::slice::from_mut(&mut node), group, *seen, &seal_key)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(due.len())
}

/// Apply the core's persisted-arrival read snapshot to display records.
pub(super) fn project_reads(
    store: &Store,
    peer_id: &str,
    views: &mut [MessageView],
) -> Result<(), String> {
    let coverage = store
        .read_coverage(peer_id)
        .map_err(|error| error.to_string())?;
    for view in views {
        if let Some(read) = coverage.read_flag(
            &view.group_id,
            &view.author,
            &view.id,
            view.edited,
            view.deleted,
        ) {
            view.read = read;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_dir;
    use super::*;

    fn view(group: u8, author: &str, id: u8) -> MessageView {
        MessageView {
            group_id: vec![group],
            author: author.into(),
            id: [id; 16],
            text: Some("text".into()),
            link_hosts: Vec::new(),
            edited: false,
            deleted: false,
            unread: false,
            forwarded: false,
            delivery: None,
            read: false,
            sent_at_ms: None,
            photo: None,
            sticker: None,
            shared_peer: None,
        }
    }

    #[test]
    fn nothing_due_never_reaches_the_network() {
        use dyapp_client::interactions::{Interaction, Kind};
        let (dir, alice, bob) = (
            temp_dir(),
            Identity::try_generate().unwrap(),
            Identity::try_generate().unwrap(),
        );
        let (alice_id, bob_id) = (alice.peer_id(), bob.peer_id());
        let mut store = super::super::open(&dir, &alice).unwrap();
        for (sender, target, device, event_id, wall_ms) in [
            (&alice_id, &bob_id, [1; 32], [1; 16], 1_000),
            (&bob_id, &alice_id, [2; 32], [2; 16], 2_000),
        ] {
            let interaction = Interaction {
                kind: Kind::Like,
                event_id,
                hlc: wall_ms << 16,
            };
            store
                .apply_interaction(sender, target, &device, interaction)
                .unwrap();
        }
        assert!(!store.send_read_receipts().unwrap());
        drop(store);
        // No network file exists, so reaching the connect step would return notConnected.
        let device = Identity::try_generate().unwrap();
        let sent = send_read_receipts(&dir, &dir, &alice, &device, &bob_id);
        assert_eq!(tauri::async_runtime::block_on(sent), Ok(0));
    }

    #[test]
    fn a_mark_covers_earlier_live_own_texts_of_its_group_only() {
        let owner = Identity::try_generate().unwrap();
        let own = owner.peer_id();
        let peer = Identity::try_generate().unwrap().peer_id();
        let mut store = Store::open(&temp_dir().join("reads.db"), &owner).unwrap();
        let mut views = vec![
            view(1, &own, 1),
            view(1, &peer, 2),
            view(1, &own, 3),
            view(1, &own, 4),
            view(2, &own, 5),
        ];
        for view in &views {
            let key = dyapp_client::store::messages::MessageKey {
                group: view.group_id.clone(),
                author: view.author.clone(),
                id: view.id,
            };
            assert!(store.record_chat_text(&key, b"text").unwrap());
        }
        assert!(store.receive_read(&[1], &peer, [3; 16]).unwrap());
        views[0].edited = true;
        project_reads(&store, &peer, &mut views).unwrap();
        let read: Vec<_> = views.iter().map(|view| view.read).collect();
        assert_eq!(read, [false, false, true, false, false]);
    }
}
