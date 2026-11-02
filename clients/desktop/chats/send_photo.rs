//! Send one photo into an open conversation. The page re-encodes the picked file, so what arrives
//! here is a bare JPEG with its preview; a chat photo never creates a message request.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::attachment::{self, Attachment};
use dyapp_client::session;
use dyapp_client::Error;
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::Status;

use crate::search::ProfileCache;

const JPEG_START: [u8; 2] = [0xFF, 0xD8];

fn sealed(
    photo: &[u8],
    preview: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(Attachment, Vec<u8>), String> {
    if !photo.starts_with(&JPEG_START) {
        return Err("unsupported".into());
    }
    crate::chat_photos::check_image(photo)?;
    let (mut attachment, blob) = attachment::seal(photo).map_err(|e| e.to_string())?;
    attachment.preview = preview;
    attachment.mime = "image/jpeg".into();
    attachment.width = width;
    attachment.height = height;
    Ok((attachment, blob))
}

/// The page's code for a node refusal; anything else is shown as it is.
pub(super) fn code(error: session::Error) -> String {
    match error {
        session::Error::Client(Error::Status(Status::NotFound)) => "peerUnavailable".into(),
        session::Error::Client(Error::Status(Status::Full)) => "full".into(),
        session::Error::Client(Error::Status(Status::TooLarge)) => "tooLarge".into(),
        error => error.to_string(),
    }
}

// ponytail: one node, so a node at its pending limit fails the send; pass `profile::reachable`
// nodes if a per-node refusal shows up in practice.
async fn send(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    (attachment, blob): &(Attachment, Vec<u8>),
) -> Result<[u8; 16], String> {
    if peer_id == identity.peer_id() {
        return Err("invalidTarget".into());
    }
    let store = super::open(dir, identity)?;
    let (mut session, mut node, seal_key) =
        super::forward::open_session(cache, root, dir, identity, device_key, store, peer_id)
            .await?;
    session
        .send_attachment_to(
            std::slice::from_mut(&mut node),
            identity,
            peer_id,
            attachment,
            blob,
            &seal_key,
        )
        .await
        .map_err(code)
}

/// Seal and send a photo through the active identity's MLS Session. Errors are `unsupported`,
/// `corrupted`, `full`, `tooLarge`, `notConnected` or a message to show.
#[expect(
    clippy::too_many_arguments,
    reason = "Tauri command names the photo and its claims"
)]
#[tauri::command]
pub(crate) async fn send_chat_photo(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, crate::search::SearchState>,
    sessions: tauri::State<'_, super::Sessions>,
    peer_id: String,
    photo: Vec<u8>,
    preview: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let cache = Arc::clone(&searching.0);
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let photo = sealed(&photo, preview, width, height)?;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(send(
            &cache, &root, &dir, &identity, &device, &peer_id, &photo,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{attachment, code, sealed, session, Error, Status};
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        RgbImage::new(3, 2).write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn only_a_jpeg_is_sealed_with_its_claims() {
        let photo = encoded(ImageFormat::Jpeg);
        let (sent, blob) = sealed(&photo, vec![1], 3, 2).unwrap();
        assert_eq!(attachment::open(&sent, &blob).unwrap(), photo);
        assert_eq!(
            (sent.mime.as_str(), sent.width, sent.height),
            ("image/jpeg", 3, 2)
        );
        assert_eq!(sent.preview, [1]);
        let refused = sealed(&encoded(ImageFormat::Png), Vec::new(), 3, 2);
        assert_eq!(refused.unwrap_err(), "unsupported");
        assert_eq!(
            sealed(&[0xFF, 0xD8, 1], Vec::new(), 3, 2).unwrap_err(),
            "corrupted"
        );
    }

    #[test]
    fn node_refusals_get_their_page_codes() {
        let refused = |status| code(session::Error::Client(Error::Status(status)));
        assert_eq!(refused(Status::Full), "full");
        assert_eq!(refused(Status::TooLarge), "tooLarge");
        assert_eq!(refused(Status::NotFound), "peerUnavailable");
        assert_ne!(refused(Status::Denied), "full");
    }
}
