//! One notice for mail that arrived while the window has no focus. It holds only a fixed text in
//! the UI language, never a name or a message, because a desktop can show it on the lock screen.

use std::path::Path;

use dyapp_identity::Identity;
use tauri::{AppHandle, Manager};

/// Unread items and unanswered likes, so own, dropped and blocked mail never counts.
fn waiting(dir: &Path, identity: &Identity) -> Result<usize, String> {
    let rows = crate::chats::chat_rows(dir, identity)?;
    let likes = crate::chats::open(dir, identity)?
        .liked_by()
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|row| row.unread_count).sum::<usize>() + likes.len())
}

/// Runs a mail pass and notifies when it left more waiting. The focus check waits on the main
/// thread, which can itself wait on the session lock held here, so it runs on its own thread.
pub(crate) fn around_pass<T>(
    app: &AppHandle,
    dir: &Path,
    identity: &Identity,
    pass: impl FnOnce() -> T,
) -> T {
    let before = waiting(dir, identity);
    let result = pass();
    let grew = matches!((before, waiting(dir, identity)), (Ok(old), Ok(new)) if new > old);
    if grew && crate::privacy::load(dir, identity).is_ok_and(|privacy| privacy.notifications) {
        let app = app.clone();
        std::thread::spawn(move || {
            let window = app.get_webview_window("main");
            if !window.is_some_and(|window| window.is_focused().unwrap_or(false)) {
                if let Some(text) = crate::tray::notice_text(&app) {
                    send(&text);
                }
            }
        });
    }
    result
}

#[cfg(unix)]
type Notify<'a> = (
    &'a str,
    u32,
    &'a str,
    &'a str,
    &'a str,
    &'a [&'a str],
    std::collections::HashMap<&'a str, zbus::zvariant::Value<'a>>,
    i32,
);

/// The `Notify` arguments: application, replaced notice, icon, summary, body, actions, hints and
/// the server's own timeout.
#[cfg(unix)]
fn notice(text: &str, replaces: u32) -> Notify<'_> {
    ("DYApp", replaces, "", text, "", &[], Default::default(), -1)
}

/// Best effort, as a desktop may have no notification server. A new notice replaces the last one.
#[cfg(unix)]
fn send(text: &str) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static LAST: AtomicU32 = AtomicU32::new(0);
    const SERVER: &str = "org.freedesktop.Notifications";
    let shown = tauri::async_runtime::block_on(async {
        let args = notice(text, LAST.load(Ordering::SeqCst));
        let path = "/org/freedesktop/Notifications";
        let bus = zbus::Connection::session().await?;
        let reply = bus
            .call_method(Some(SERVER), path, Some(SERVER), "Notify", &args)
            .await?;
        let id: u32 = reply.body().deserialize()?;
        Ok::<_, zbus::Error>(id)
    });
    if let Ok(id) = shown {
        LAST.store(id, Ordering::SeqCst);
    }
}

// ponytail: Windows shows no notice yet; add the toast API when that client ships.
#[cfg(not(unix))]
fn send(_text: &str) {}

#[cfg(all(test, unix))]
mod tests {
    use super::notice;

    #[test]
    fn a_notice_carries_only_the_fixed_text() {
        let (application, replaces, icon, summary, body, actions, hints, _) =
            notice("New message", 7);
        assert_eq!((application, replaces, icon), ("DYApp", 7, ""));
        assert_eq!((summary, body), ("New message", ""));
        assert!(actions.is_empty() && hints.is_empty());
    }
}
