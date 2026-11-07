//! Opt-in read receipts of one open chat; see docs/design/read-receipts.md.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use super::MessageView;
use crate::profile;

/// The latest partner text of each group that still owes a receipt. A request owes none: only an
/// accepted conversation may learn when the chat was opened.
fn due_reads(store: &Store, peer_id: &str) -> Result<Vec<(Vec<u8>, [u8; 16])>, String> {
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
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    for (group, seen) in &due {
        session
            .mark_read(std::slice::from_mut(&mut node), group, *seen, &seal_key)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(due.len())
}

/// Mark own messages up to the peer's read mark of their group; see [`covered`].
pub(super) fn project_reads(
    store: &Store,
    peer_id: &str,
    own: &str,
    views: &mut [MessageView],
) -> Result<(), String> {
    let mut marks = HashMap::new();
    for view in views.iter() {
        if !marks.contains_key(&view.group_id) {
            let mark = store
                .read_up_to(&view.group_id, peer_id)
                .map_err(|e| e.to_string())?;
            marks.insert(view.group_id.clone(), mark);
        }
    }
    covered(&marks, own, views);
    Ok(())
}

/// A mark covers its text and every earlier own one of the group. A mark whose text was cleared
/// here marks nothing, and it says nothing about an edit made after reading or a deleted text.
fn covered(marks: &HashMap<Vec<u8>, Option<[u8; 16]>>, own: &str, views: &mut [MessageView]) {
    let mut reached = HashSet::new();
    for view in views.iter_mut().rev().filter(|view| view.author == own) {
        if marks[&view.group_id] == Some(view.id) {
            reached.insert(view.group_id.clone());
        }
        view.read = reached.contains(&view.group_id) && !view.edited && !view.deleted;
    }
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
        let (dir, alice, bob) = (temp_dir(), Identity::generate(), Identity::generate());
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
        let device = Identity::generate();
        let sent = send_read_receipts(&dir, &dir, &alice, &device, &bob_id);
        assert_eq!(tauri::async_runtime::block_on(sent), Ok(0));
    }

    #[test]
    fn a_mark_covers_earlier_live_own_texts_of_its_group_only() {
        let mut views = vec![
            view(1, "me", 1),
            view(1, "peer", 2),
            view(1, "me", 3),
            view(1, "me", 4),
            view(2, "me", 5),
        ];
        views[0].edited = true;
        let marks = HashMap::from([(vec![1], Some([3; 16])), (vec![2], None)]);
        covered(&marks, "me", &mut views);
        let read: Vec<_> = views.iter().map(|view| view.read).collect();
        assert_eq!(read, [false, false, true, false, false]);
    }
}
