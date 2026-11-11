//! Keep signature validation separate from atomic local deny reload and purge.
use super::{DenyStore, HashSet, PeerId, PoisonError, Service, SignedRecord};

fn invalid_signature(peer: &PeerId) {
    tracing::warn!(%peer, "deny list with a bad signature dropped");
}

/// Stores a received deny list only when the sending node signed it.
pub(super) fn keep(deny: &DenyStore, peer: &PeerId, record: &SignedRecord) {
    let Some(list) = crate::deny::verify(record, peer) else {
        return invalid_signature(peer);
    };
    match deny.keep_received(&peer.to_string(), list.time, record) {
        Ok(true) => received(peer, list.entries.len()),
        Ok(false) => {}
        Err(error) => tracing::error!(%peer, %error, "deny list not stored"),
    }
}

fn received(peer: &PeerId, entries: usize) {
    tracing::info!(%peer, entries, "deny list received");
}

fn hash(entry: &str) -> Option<Vec<u8>> {
    if entry.len() != 64 || !entry.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&entry[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

fn purge(service: &Service, entries: &HashSet<String>) -> bool {
    let Some(media) = &service.media else {
        return true;
    };
    let mut purged = true;
    for entry in entries {
        let Some(hash) = hash(entry) else {
            continue;
        };
        if let Err(error) = media.purge(&hash) {
            tracing::error!(hash = %entry, %error, "deny-listed media not purged");
            purged = false;
        }
    }
    purged
}

fn apply(service: &Service, entries: HashSet<String>) -> bool {
    let mut old = service.deny.write().unwrap_or_else(PoisonError::into_inner);
    let added = entries.difference(&old).count();
    let removed = old.difference(&entries).count();
    // Replica writes hold the read lock until stored: keep this lock through purge.
    let purged = purge(service, &entries);
    tracing::info!(entries = entries.len(), added, removed, "deny list loaded");
    *service
        .shared_deny
        .write()
        .unwrap_or_else(PoisonError::into_inner) = service
        .deny_signer
        .as_ref()
        .map(|key| crate::deny::sign(key, &entries));
    *old = entries;
    purged
}

/// Retry a failed purge even when the persisted deny-list version has not changed.
pub(super) fn reload(service: &Service, deny: &DenyStore, version: &mut Option<i64>) {
    let current = match deny.version() {
        Ok(current) if Some(current) == *version => return,
        Ok(current) => current,
        Err(error) => return tracing::error!(%error, "deny list not checked"),
    };
    if let Some(entries) = load(deny) {
        *version = apply(service, entries).then_some(current);
    }
}

fn load(deny: &DenyStore) -> Option<HashSet<String>> {
    match deny.entries() {
        Ok(entries) => Some(entries),
        Err(error) => {
            tracing::error!(%error, "deny list not loaded, the old one stays");
            None
        }
    }
}
