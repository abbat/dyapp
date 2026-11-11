use super::{ChatKeyKind, ChatKeyStore};
use crate::identity_slots::{keys, write, Slot};
use crate::store::Store;
use dyapp_identity::Identity;
use std::fs;
use zeroize::Zeroizing;

fn read(slot: &Slot, kind: ChatKeyKind) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    keys::wrapped_bytes(&slot.dir.join(kind.file_name()))
}

fn snapshot(slot: &Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String> {
    let path = slot.dir.join("client.db");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err("Chat database is not a regular file".into()),
        Err(error) => return Err(error.to_string()),
    }
    Store::open(&path, identity)
        .and_then(|store| store.sealed_device())
        .map_err(|error| error.to_string())
}

pub(super) fn prepare(
    slot: &Slot,
    identity: &Identity,
    wrapping: &dyn ChatKeyStore,
) -> Result<(), String> {
    let owner = required(slot, ChatKeyKind::Identity, wrapping)?;
    if owner != identity.secret() {
        return Err("The wrapping store does not match this identity".into());
    }
    let device = read(slot, ChatKeyKind::Device)?;
    let state = read(slot, ChatKeyKind::State)?;
    if let Some(sealed) = snapshot(slot, identity)? {
        return verify_snapshot(slot, device, state, &sealed, wrapping);
    }
    let existing = [(ChatKeyKind::Device, device), (ChatKeyKind::State, state)];
    for (kind, bytes) in &existing {
        if let Some(bytes) = bytes {
            wrapping.unwrap(slot, *kind, bytes)?;
        }
    }
    for (kind, bytes) in existing {
        if bytes.is_none() {
            fresh(slot, kind, wrapping)?;
        }
    }
    Ok(())
}

fn verify_snapshot(
    slot: &Slot,
    device: Option<Zeroizing<Vec<u8>>>,
    state: Option<Zeroizing<Vec<u8>>>,
    sealed: &[u8],
    wrapping: &dyn ChatKeyStore,
) -> Result<(), String> {
    let (Some(device), Some(state)) = (device, state) else {
        return Err("An initialized chat database is missing device or state keys".into());
    };
    let device = wrapping.unwrap(slot, ChatKeyKind::Device, &device)?;
    let state = wrapping.unwrap(slot, ChatKeyKind::State, &state)?;
    let saved =
        dyapp_messaging::mls::Device::open(sealed, &state).map_err(|error| error.to_string())?;
    if saved.device_key().map_err(|error| error.to_string())?
        != Identity::from_secret(&device).public_key()
    {
        return Err("The saved chat device key changed".into());
    }
    Ok(())
}

fn fresh(slot: &Slot, kind: ChatKeyKind, wrapping: &dyn ChatKeyStore) -> Result<(), String> {
    let mut secret = Zeroizing::new([0; 32]);
    getrandom::fill(&mut *secret).map_err(|error| error.to_string())?;
    let wrapped = wrapping.wrap(slot, kind, &secret)?;
    if wrapped.len() > keys::MAX_WRAPPED_BYTES {
        return Err("Wrapped chat key exceeds the storage limit".into());
    }
    if wrapping.unwrap(slot, kind, &wrapped)? != secret {
        return Err("Wrapped chat key does not reopen".into());
    }
    write::fresh(&slot.dir.join(kind.file_name()), &wrapped)?;
    if required(slot, kind, wrapping)? != secret {
        return Err("Saved chat key does not reopen".into());
    }
    Ok(())
}

pub(super) fn required(
    slot: &Slot,
    kind: ChatKeyKind,
    wrapping: &dyn ChatKeyStore,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let bytes = read(slot, kind)?.ok_or("Chat key is missing")?;
    wrapping.unwrap(slot, kind, &bytes)
}
