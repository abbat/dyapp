//! Place-picker IPC wrappers; command names remain stable across native clients.
use crate::profile::Place;

/// Countries with places for the place picker; the page names them.
#[tauri::command]
pub(super) fn place_countries() -> Vec<&'static str> {
    dyapp_client::places::countries()
}

/// The place `id` of `country` in `lang`; see `dyapp_client::places::place`.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(super) fn place_name(country: String, id: u32, lang: String) -> Option<Place> {
    dyapp_client::places::place(&country, id, &lang).map(Into::into)
}

/// Places of `country` whose name starts with `prefix`; see `dyapp_client::places::search`.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(super) fn search_places(
    country: String,
    prefix: String,
    lang: String,
    limit: usize,
) -> Vec<Place> {
    let found = dyapp_client::places::search(&country, &prefix, &lang, limit);
    found.into_iter().map(Into::into).collect()
}
