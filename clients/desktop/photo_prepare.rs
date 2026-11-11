//! Shared photo preparation for the profile and chat commands. The core picks the decode size,
//! the page decodes the picked file upright into straight RGBA at that size, and the core fits
//! and encodes it, so EXIF, GPS, colour profiles and appended bytes never leave the computer.

use dyapp_client::photo::prepare::{self, PreparedPhoto, Purpose};
use tauri::http::HeaderMap;
use tauri::ipc::InvokeBody;

/// Which shared preparation policy the page asks for.
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PhotoPurpose {
    Profile,
    Chat,
}

impl From<PhotoPurpose> for Purpose {
    fn from(value: PhotoPurpose) -> Self {
        match value {
            PhotoPurpose::Profile => Self::Profile,
            PhotoPurpose::Chat => Self::Chat,
        }
    }
}

/// Upright size the page decodes the picked file to.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Plan {
    width: u32,
    height: u32,
}

/// Largest picked file the page may read.
#[tauri::command]
pub(crate) fn photo_picked_byte_limit() -> u64 {
    prepare::MAX_PICKED_BYTES
}

/// Decode size for a picked file. Errors are `unsupported` or `tooLarge`.
#[tauri::command]
pub(crate) fn photo_decode_plan(
    mime: &str,
    width: u32,
    height: u32,
    bytes: u64,
    purpose: PhotoPurpose,
) -> Result<Plan, String> {
    let plan = prepare::decode_plan(mime, width, height, bytes, purpose.into())
        .map_err(|e| e.to_string())?;
    Ok(Plan {
        width: plan.width,
        height: plan.height,
    })
}

/// A header the page sets on a raw photo request; a missing one is `unsupported`.
pub(crate) fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "unsupported".to_string())
}

/// The decoded RGBA body with its `dyapp-width` and `dyapp-height` headers.
pub(crate) fn pixels(
    headers: &HeaderMap,
    body: &InvokeBody,
) -> Result<(Vec<u8>, u32, u32), String> {
    let side = |name| {
        header(headers, name)?
            .parse::<u32>()
            .map_err(|_| "unsupported".to_string())
    };
    let (width, height) = (side("dyapp-width")?, side("dyapp-height")?);
    match body {
        InvokeBody::Raw(rgba) => Ok((rgba.clone(), width, height)),
        InvokeBody::Json(_) => Err("unsupported".into()),
    }
}

/// Fit and encode decoded pixels. Errors are `tooLarge` or `unsupported`.
pub(crate) fn prepared(
    rgba: &[u8],
    width: u32,
    height: u32,
    purpose: Purpose,
) -> Result<PreparedPhoto, String> {
    prepare::prepare_photo(rgba, width, height, purpose).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{photo_decode_plan, pixels, prepared, HeaderMap, InvokeBody, PhotoPurpose, Plan};
    use dyapp_client::photo::prepare::Purpose;

    #[test]
    fn the_core_picks_each_purpose_decode_size() {
        let plan = |purpose| photo_decode_plan("image/jpeg", 4000, 3000, 1, purpose);
        let profile = Plan {
            width: 2048,
            height: 1536,
        };
        assert_eq!(plan(PhotoPurpose::Profile), Ok(profile));
        assert_eq!(
            plan(PhotoPurpose::Chat),
            Ok(Plan {
                width: 2560,
                height: 1920
            })
        );
        let gif = photo_decode_plan("image/gif", 4, 4, 1, PhotoPurpose::Chat);
        assert_eq!(gif.unwrap_err(), "unsupported");
        let huge = photo_decode_plan("image/png", 9000, 9000, 1, PhotoPurpose::Chat);
        assert_eq!(huge.unwrap_err(), "tooLarge");
    }

    #[test]
    fn decoded_pixels_become_a_bare_jpeg() {
        let photo = prepared(&[200; 8 * 6 * 4], 8, 6, Purpose::Chat).unwrap();
        assert!(photo.full.starts_with(&[0xFF, 0xD8]));
        assert!(photo.thumbnail.starts_with(&[0xFF, 0xD8]));
        assert_eq!((photo.width, photo.height), (8, 6));
        let short = prepared(&[0; 7], 8, 6, Purpose::Chat);
        assert_eq!(short.unwrap_err(), "unsupported");
        let wide = prepared(&[0; 4], 2049, 1, Purpose::Profile);
        assert_eq!(wide.unwrap_err(), "tooLarge");
    }

    #[test]
    fn a_raw_body_needs_its_size_headers() {
        let mut headers = HeaderMap::new();
        let body = InvokeBody::Raw(vec![1, 2, 3, 4]);
        assert_eq!(pixels(&headers, &body).unwrap_err(), "unsupported");
        headers.insert("dyapp-width", "1".parse().unwrap());
        headers.insert("dyapp-height", "1".parse().unwrap());
        assert_eq!(pixels(&headers, &body).unwrap(), (vec![1, 2, 3, 4], 1, 1));
        let json = InvokeBody::Json(serde_json::Value::Null);
        assert_eq!(pixels(&headers, &json).unwrap_err(), "unsupported");
        headers.insert("dyapp-width", "wide".parse().unwrap());
        assert_eq!(pixels(&headers, &body).unwrap_err(), "unsupported");
    }
}
