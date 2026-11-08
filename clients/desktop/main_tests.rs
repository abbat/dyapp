use super::identity_runtime::ready_result;
use super::{drain_and_clear, search, IdentityState, NetworkState};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn identity_transition_clears_profiles_only_after_old_work_drains() {
    let state = IdentityState::new();
    state
        .start_transition()
        .unwrap()
        .commit(Some((
            Arc::new(Identity::generate()),
            PathBuf::from("first"),
        )))
        .unwrap();
    let lease = state.lease().unwrap();
    let owner = Identity::generate();
    let verified = dyapp_profile::verify(
        &Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&owner),
    )
    .unwrap();
    let searching = Arc::new(search::SearchState::default());
    assert!(searching.0.lock().unwrap().accept_if_current(0, verified));
    let checks = Arc::new(NetworkState::default());
    let transition = state.start_transition().unwrap();
    let worker = {
        let searching = Arc::clone(&searching);
        let checks = Arc::clone(&checks);
        tokio::spawn(async move { drain_and_clear(transition, &searching, &checks).await })
    };
    tokio::task::yield_now().await;
    assert!(!worker.is_finished());
    assert_eq!(searching.0.lock().unwrap().snapshot().1.all().len(), 1);
    drop(lease);
    worker.await.unwrap().unwrap().commit(None).unwrap();
    assert_eq!(searching.0.lock().unwrap().snapshot().1.all(), []);
}

#[test]
fn backup_path_is_only_returned_for_file_storage() {
    let dir = Path::new("profile");
    let file = ready_result("file", "peer".into(), dir);
    assert_eq!(
        file["keyPath"],
        dir.join("identity.key").display().to_string()
    );
    let system = ready_result("system", "peer".into(), dir);
    assert!(!system.contains_key("keyPath"));
}

#[test]
fn configured_window_is_visible_and_local() {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let app = &context.config().app;
    assert_eq!(app.windows.len(), 1);
    // Hidden until `window::restore` has placed it, then shown.
    assert!(!app.windows[0].visible);
    assert_eq!(
        (app.windows[0].min_width, app.windows[0].min_height),
        (Some(400.0), Some(560.0))
    );
    assert_eq!(app.windows[0].title, "DYApp");
    assert!(matches!(app.windows[0].url, tauri::WebviewUrl::App(_)));
    // No capabilities: the page can call only the app's own command, no plugin APIs.
    assert_eq!(
        app.security.capabilities,
        Vec::<tauri::utils::config::CapabilityEntry>::new()
    );
}
