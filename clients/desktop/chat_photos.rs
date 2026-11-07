//! The original of one chat photo, fetched on a tap. The page keeps it for the open chat, and this
//! device keeps it sealed in [`originals`] up to the limit in Settings, so the same command answers
//! from there after a restart or the nodes' retention. The automatic path asks [`chat_photo_auto`]
//! and then fetches through the same command.

use crate::{chats, profile};
use dyapp_client::attachment::{self, Attachment, AutoDownload};
use dyapp_client::store::{originals, Conversation};
use dyapp_client::Error;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::Status;
use image::ImageFormat;
use std::io::Cursor;
use std::path::Path;

/// The most pixels the page may decode. A 6 MiB original can claim gigabytes of decoded image, and
/// the header states the size before any decoding.
const MAX_PIXELS: u64 = 64_000_000;

fn stored(
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
    author: &str,
    id: &[u8; 16],
) -> Result<Attachment, String> {
    chats::open_visible_chat(dir, identity, peer_id)?
        .current_chat_attachments(peer_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|photo| photo.key.author == author && photo.key.id == *id)
        .map(|photo| photo.message.attachment)
        .ok_or_else(|| "notFound".into())
}

fn fits(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// Refuses anything but a JPEG or PNG whose header is within the bound.
pub(crate) fn check_image(bytes: &[u8]) -> Result<(), String> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Jpeg | ImageFormat::Png)) {
        return Err("corrupted".into());
    }
    let (width, height) = reader.into_dimensions().map_err(|_| "corrupted")?;
    if !fits(width, height) {
        return Err("corrupted".into());
    }
    Ok(())
}

/// The page's code for a failed fetch. Anything else is a transport or node failure to retry.
pub(crate) fn code(error: Error) -> String {
    match error {
        Error::Status(Status::NotFound) => "notFound".into(),
        Error::Attachment(_) => "corrupted".into(),
        error => error.to_string(),
    }
}

async fn original(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
    author: &str,
    id: &[u8; 16],
) -> Result<Vec<u8>, String> {
    let photo = stored(dir, identity, peer_id, author, id)?;
    let seal_key = crate::sealing_key(root, dir, &chats::open(dir, identity)?)?;
    if let Some(bytes) = originals::read(dir, &seal_key, &photo) {
        return Ok(bytes);
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut nodes = profile::reachable(&active).await?;
    if nodes.is_empty() {
        return Err("notConnected".into());
    }
    let bytes = attachment::fetch(&mut nodes, &photo).await.map_err(code)?;
    check_image(&bytes)?;
    // The photo is on screen either way, and a later tap fetches it again while nodes hold it.
    if let Err(error) = keep(dir, identity, &seal_key, &photo, &bytes) {
        eprintln!("original not kept: {error}");
    }
    Ok(bytes)
}

pub(crate) mod cache;

fn keep(
    dir: &Path,
    identity: &Identity,
    seal_key: &[u8; 32],
    photo: &Attachment,
    bytes: &[u8],
) -> Result<(), String> {
    let store = chats::open(dir, identity)?;
    if store.kept_originals_mib().map_err(|e| e.to_string())? > 0 {
        originals::keep(dir, seal_key, photo, bytes).map_err(|e| e.to_string())?;
        store.sweep_originals(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Answers from a kept original, or downloads, opens and checks one photo; a tap may fetch a
/// stranger's. Errors are `notFound` when
/// no node holds it, `corrupted` when it fails its checks, `notConnected` or a retryable message.
#[tauri::command]
pub(crate) async fn chat_photo_original(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    peer_id: String,
    author: String,
    id: [u8; 16],
) -> Result<tauri::ipc::Response, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(original(&root, &dir, &identity, &peer_id, &author, &id))
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// NetworkManager's connection is known or guessed unmetered. Anything else, a missing service
/// included, counts as metered.
#[cfg(unix)]
pub(crate) async fn metered() -> bool {
    const MANAGER: &str = "org.freedesktop.NetworkManager";
    let state = async {
        let bus = zbus::Connection::system().await?;
        let reply = bus
            .call_method(
                Some(MANAGER),
                "/org/freedesktop/NetworkManager",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(MANAGER, "Metered"),
            )
            .await?;
        let value: zbus::OwnedVariant = reply.body().deserialize()?;
        u32::try_from(value).map_err(zbus::Error::from)
    };
    // NMMetered: NO is 2 and GUESS_NO is 4.
    !matches!(state.await, Ok(2 | 4))
}

/// No source of this yet: an unknown network counts as metered.
#[cfg(not(unix))]
pub(crate) async fn metered() -> bool {
    true
}

/// Whether the open chat's originals may download without a tap now. Called again on each mail
/// pass, so a switch to an unmetered network picks up what waited.
#[tauri::command]
pub(crate) async fn chat_photo_auto(
    state: tauri::State<'_, crate::IdentityState>,
    peer_id: String,
) -> Result<bool, String> {
    let (mode, contact) = {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        let store = chats::open_visible_chat(dir, identity, &peer_id)?;
        let mode = store.auto_download().map_err(|e| e.to_string())?;
        let conversation = store.conversation(&peer_id).map_err(|e| e.to_string())?;
        (mode, conversation == Conversation::Open)
    };
    Ok(attachment::auto_download(mode, metered().await, contact))
}

const MODES: [AutoDownload; 3] = [
    AutoDownload::ByNetwork,
    AutoDownload::Always,
    AutoDownload::OnTap,
];

fn mode_name(mode: AutoDownload) -> &'static str {
    match mode {
        AutoDownload::ByNetwork => "byNetwork",
        AutoDownload::Always => "always",
        AutoDownload::OnTap => "onTap",
    }
}

fn parse_mode(name: &str) -> Result<AutoDownload, String> {
    MODES
        .into_iter()
        .find(|mode| mode_name(*mode) == name)
        .ok_or_else(|| "unknown mode".into())
}

/// Settings → Photos: this identity's choice on this device.
#[tauri::command]
pub(crate) fn auto_download_mode(
    state: tauri::State<'_, crate::IdentityState>,
) -> Result<&'static str, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    chats::open(dir, identity)?
        .auto_download()
        .map(mode_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn set_auto_download_mode(
    state: tauri::State<'_, crate::IdentityState>,
    mode: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    chats::open(dir, identity)?
        .set_auto_download(parse_mode(&mode)?)
        .map_err(|e| e.to_string())
}

/// Settings → Photos: the limit for originals kept on this device, in MiB.
#[tauri::command]
pub(crate) fn kept_originals_mib(
    state: tauri::State<'_, crate::IdentityState>,
) -> Result<u32, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    chats::open(dir, identity)?
        .kept_originals_mib()
        .map_err(|e| e.to_string())
}

/// A lower limit erases what no longer fits at once.
#[tauri::command]
pub(crate) fn set_kept_originals_mib(
    state: tauri::State<'_, crate::IdentityState>,
    mib: u32,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = chats::open(dir, identity)?;
    store
        .set_kept_originals_mib(mib)
        .map_err(|e| e.to_string())?;
    store.sweep_originals(dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        check_image, code, fits, mode_name, parse_mode, Error, ImageFormat, Status, MODES,
    };
    use image::RgbImage;
    use std::io::Cursor;

    #[test]
    fn only_a_real_image_within_the_bound_passes() {
        let mut png = Cursor::new(Vec::new());
        RgbImage::new(3, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        assert_eq!(check_image(png.get_ref()), Ok(()));
        assert_eq!(check_image(b"not an image"), Err("corrupted".into()));
        assert!(fits(8_000, 8_000));
        assert!(!fits(8_001, 8_000));
        assert!(!fits(u32::MAX, u32::MAX));
    }

    #[test]
    fn every_mode_keeps_its_name_for_the_page() {
        for mode in MODES {
            assert_eq!(parse_mode(mode_name(mode)), Ok(mode));
        }
        assert!(parse_mode("sometimes").is_err());
    }

    #[test]
    fn a_missing_or_broken_original_gets_its_own_code() {
        assert_eq!(code(Error::Status(Status::NotFound)), "notFound");
        assert_eq!(code(Error::Attachment("x")), "corrupted");
        assert_ne!(code(Error::Timeout), "notFound");
    }
}
