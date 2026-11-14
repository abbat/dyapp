//! One notice for mail that arrived while the window has no focus. By default it holds only a
//! fixed text in the UI language, because a desktop can show it on the lock screen; the identity's
//! settings can add the sender's name and the message.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(test)]
use dyapp_client::store::Conversation;
use dyapp_client::store::{ChatRow, NoticeContent};
use dyapp_identity::Identity;
use tauri::{AppHandle, Manager};

#[cfg(test)]
use dyapp_client::mail_notices::TEXT_CHARS as BODY_CHARS;

/// Rows and unanswered likes without blocked or muted peers, so own, dropped, blocked and muted mail
/// never counts.
fn waiting(dir: &Path, identity: &Identity) -> Result<(Vec<ChatRow>, usize), String> {
    let store = crate::chats::open(dir, identity)?;
    let quiet = |peer: &str| -> Result<bool, String> {
        Ok(store.blocked(peer).map_err(|e| e.to_string())?
            || store.muted(peer).map_err(|e| e.to_string())?)
    };
    let mut rows = Vec::new();
    for row in store.chat_rows().map_err(|e| e.to_string())? {
        if !quiet(&row.peer_id)? {
            rows.push(row);
        }
    }
    let mut likes = 0;
    for peer in store.liked_by().map_err(|e| e.to_string())? {
        likes += usize::from(!quiet(&peer)?);
    }
    Ok((rows, likes))
}

/// Resolve UI labels on the detached send thread, without waits on the session-owning thread.
#[derive(Default)]
struct Preview(dyapp_client::mail_notices::Preview);

impl Preview {
    // A server may interpret markup. A server without body-markup shows these entities.
    fn render(self, sticker_label: &str) -> String {
        self.0
            .plain_text(sticker_label)
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

#[cfg(test)]
fn body(row: &ChatRow) -> Preview {
    Preview(dyapp_client::mail_notices::body(row))
}

/// Core selects allowed additions; the adapter resolves only the local display name.
fn content(
    mode: NoticeContent,
    before: &[ChatRow],
    after: &[ChatRow],
    name: impl FnOnce(&str) -> String,
) -> (String, Preview) {
    dyapp_client::mail_notices::content(mode, before, after)
        .map_or_else(Default::default, |selected| {
            (name(selected.peer_id), Preview(selected.preview))
        })
}

/// The content of the notice a mail pass calls for, if any.
fn due(
    app: &AppHandle,
    dir: &Path,
    identity: &Identity,
    before: Result<(Vec<ChatRow>, usize), String>,
    after: Result<(Vec<ChatRow>, usize), String>,
) -> Option<(String, Preview)> {
    let ((old, old_likes), (new, new_likes)) = (before.ok()?, after.ok()?);
    let unread = |rows: &[ChatRow]| rows.iter().map(|row| row.unread_count).sum::<usize>();
    let privacy = crate::privacy::load(dir, identity).ok()?;
    if unread(&new) + new_likes <= unread(&old) + old_likes || !privacy.notifications {
        return None;
    }
    let name = |peer_id: &str| {
        let search = app.state::<crate::search::SearchState>();
        crate::chats::open(dir, identity)
            .and_then(|store| {
                crate::chats::names::names(&search.0, &store, vec![peer_id.to_owned()])
            })
            .ok()
            .and_then(|mut names| names.pop())
            .map_or_else(String::new, |view| view.name)
    };
    Some(content(privacy.notice_content, &old, &new, name))
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
    if let Some((name, body)) = due(app, dir, identity, before, waiting(dir, identity)) {
        let app = app.clone();
        std::thread::spawn(move || deliver(&app, &name, body));
    }
    result
}

fn deliver(app: &AppHandle, name: &str, body: Preview) {
    let window = app.get_webview_window("main");
    if window.is_some_and(|window| window.is_focused().unwrap_or(false)) {
        return;
    }
    if let Some((open, fixed, sticker)) = crate::tray::notice_labels(app) {
        let body = body.render(&sticker);
        send(
            app,
            if name.is_empty() { &fixed } else { name },
            &body,
            &open,
        );
    }
}

#[cfg(target_os = "linux")]
const SERVER: &str = "org.freedesktop.Notifications";
#[cfg(target_os = "linux")]
const PATH: &str = "/org/freedesktop/Notifications";
/// The notice's own connection: a server can tie a notice to it, and send it the click alone.
#[cfg(target_os = "linux")]
static BUS: std::sync::OnceLock<zbus::Connection> = std::sync::OnceLock::new();
#[cfg(target_os = "linux")]
static LAST: AtomicU32 = AtomicU32::new(0);

/// Opens the connection for notices and shows the window when the last one is clicked. A desktop
/// without a session bus shows neither.
#[cfg(target_os = "linux")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        use zbus::export::futures_core::Stream;
        let listen = async {
            let bus = zbus::Connection::session().await?;
            let _ = BUS.set(bus.clone());
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(SERVER)
                .member("ActionInvoked")
                .path(PATH)
                .build()?;
            zbus::MessageStream::for_match_rule(rule, &bus, None).await
        };
        let Ok(mut stream) = listen.await else { return };
        while let Some(Ok(message)) =
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut stream).poll_next(cx)).await
        {
            let Ok((id, _)) = message.body().deserialize::<(u32, String)>() else {
                continue;
            };
            if id != 0 && id == LAST.load(Ordering::SeqCst) {
                crate::tray::show(&app);
            }
        }
    });
}

/// The identity toasts are posted under; the same as the app's bundle identifier.
#[cfg(target_os = "windows")]
const AUMID: &str = "com.dyapp";

/// The last toast, kept so its click handler lives as long as it can be clicked.
#[cfg(target_os = "windows")]
static SHOWN: std::sync::Mutex<Option<windows::UI::Notifications::ToastNotification>> =
    std::sync::Mutex::new(None);

/// The portable app has no Start menu shortcut to carry its identity, so the identity is named
/// under the user's own registry key; without it Windows drops the toasts.
#[cfg(target_os = "windows")]
fn register() -> windows::core::Result<()> {
    windows_registry::CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{AUMID}"))?
        .set_string("DisplayName", "DYApp")
}

/// Best effort: a failed registration leaves the app without notices.
#[cfg(target_os = "windows")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn watch(_app: AppHandle) {
    let _ = register();
}

#[cfg(target_os = "macos")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn watch(app: AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let restore = app.clone();
    let _ = app.notification().on_action(move |action| {
        if matches!(action.action_id(), "tap" | "open") {
            crate::tray::show(&restore);
        }
    });
}

#[cfg(target_os = "linux")]
type Notify<'a> = (
    &'a str,
    u32,
    &'a str,
    &'a str,
    &'a str,
    &'a [&'a str],
    std::collections::HashMap<&'a str, zbus::Variant<'a>>,
    i32,
);

/// The `Notify` arguments: application, replaced notice, icon, summary, body, actions, hints and
/// the server's own timeout.
#[cfg(target_os = "linux")]
fn notice<'a>(
    summary: &'a str,
    body: &'a str,
    actions: &'a [&'a str],
    replaces: u32,
) -> Notify<'a> {
    let hints = std::collections::HashMap::default();
    ("DYApp", replaces, "", summary, body, actions, hints, -1)
}

/// Best effort, as a desktop may have no notification server. A new notice replaces the last one,
/// and a click on it runs the `default` action.
#[cfg(target_os = "linux")]
fn send(_app: &AppHandle, summary: &str, body: &str, open: &str) {
    let Some(bus) = BUS.get() else { return };
    let shown = tauri::async_runtime::block_on(async {
        let actions = ["default", open];
        let args = notice(summary, body, &actions, LAST.load(Ordering::SeqCst));
        let reply = bus
            .call_method(Some(SERVER), PATH, Some(SERVER), "Notify", &args)
            .await?;
        let id: u32 = reply.body().deserialize()?;
        Ok::<_, zbus::Error>(id)
    });
    if let Ok(id) = shown {
        LAST.store(id, Ordering::SeqCst);
    }
}

/// Best effort. A new toast replaces the last one, and a click on it or its button, while the app
/// runs, shows the window.
#[cfg(target_os = "windows")]
fn send(app: &AppHandle, summary: &str, body: &str, open: &str) {
    let app = app.clone();
    let _ = toast(summary, &plain_body(body), open, "mail", move || {
        crate::tray::show(&app);
    });
}

/// The text goes in as text nodes, so nothing in it is read as markup.
#[cfg(target_os = "windows")]
fn toast(
    summary: &str,
    body: &str,
    open: &str,
    tag: &str,
    click: impl Fn() + Send + 'static,
) -> windows::core::Result<()> {
    use windows::core::{h, Interface, HSTRING};
    use windows::Data::Xml::Dom::{XmlDocument, XmlElement};
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    let xml = XmlDocument::new()?;
    xml.LoadXml(&HSTRING::from(concat!(
        r#"<toast><visual><binding template="ToastGeneric"><text/><text/></binding></visual>"#,
        r#"<actions><action arguments="open" content=""/></actions></toast>"#,
    )))?;
    let texts = xml.GetElementsByTagName(h!("text"))?;
    texts
        .Item(0)?
        .AppendChild(&xml.CreateTextNode(&HSTRING::from(summary))?)?;
    texts
        .Item(1)?
        .AppendChild(&xml.CreateTextNode(&HSTRING::from(body))?)?;
    let action = xml
        .GetElementsByTagName(h!("action"))?
        .Item(0)?
        .cast::<XmlElement>()?;
    action.SetAttribute(h!("content"), &HSTRING::from(open))?;
    let notice = ToastNotification::CreateToastNotification(&xml)?;
    notice.SetTag(&HSTRING::from(tag))?;
    notice.SetGroup(h!("dyapp"))?;
    notice.Activated(&TypedEventHandler::new(move |_, _| {
        click();
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&notice)?;
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(notice);
    }
    Ok(())
}

/// macOS and Windows notification bodies are plain text; reverse only the three escapes made by
/// Preview.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn plain_body(escaped: &str) -> String {
    escaped
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(target_os = "macos")]
fn send(app: &AppHandle, summary: &str, body: &str, open: &str) {
    use tauri_plugin_notification::{Action, ActionType, NotificationExt};
    let notifications = app.notification();
    let actions = ActionType::builder("mail")
        .actions(vec![Action::builder("open", open).build()])
        .build();
    let result = notifications
        .register_action_types(vec![actions])
        .and_then(|()| {
            notifications
                .builder()
                .id(1)
                .title(summary)
                .body(plain_body(body))
                .action_type_id("mail")
                .show()
        });
    if let Err(error) = result {
        eprintln!("Desktop notification dispatch failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_client::store::messages::{ChatMessage, CurrentMessage, MessageKey};

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn macos_notice_preserves_literal_text_without_html_entities() {
        let text = "<b>literal</b> & &lt;unchanged&gt;";
        let rendered =
            Preview(dyapp_client::mail_notices::Preview::Text(text.into())).render("Sticker");
        assert_eq!(plain_body(&rendered), text);
    }

    /// Windows itself must hold the posted toast with its markup-like text intact; a successful
    /// `Show` alone proves nothing.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_windows_toast_reaches_the_action_center_as_text() {
        use windows::core::{h, HSTRING};
        use windows::UI::Notifications::ToastNotificationManager;
        register().unwrap();
        let body = "<b>literal</b> & text";
        toast("Peer", body, "Open", "unit-test", || {}).unwrap();
        let history = ToastNotificationManager::History().unwrap();
        let posted = history.GetHistoryWithId(&HSTRING::from(AUMID)).unwrap();
        let found = posted
            .into_iter()
            .find(|notice| notice.Tag().unwrap() == "unit-test");
        let xml = found
            .expect("toast in the history")
            .Content()
            .unwrap()
            .GetXml()
            .unwrap();
        history
            .RemoveGroupedTagWithId(h!("unit-test"), h!("dyapp"), &HSTRING::from(AUMID))
            .unwrap();
        assert!(
            xml.to_string()
                .contains("&lt;b&gt;literal&lt;/b&gt; &amp; text"),
            "{xml}"
        );
        assert!(xml.to_string().contains(r#"content="Open""#), "{xml}");
    }

    fn row(peer: &str, state: Conversation, text: &str, unread_count: usize) -> ChatRow {
        let last_message = ChatMessage {
            key: MessageKey {
                group: vec![1],
                author: peer.to_owned(),
                id: [1; 16],
            },
            current: CurrentMessage {
                text: Some(text.as_bytes().to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc: None,
            unread: true,
            forwarded: false,
            delivery: None,
        };
        ChatRow {
            peer_id: peer.to_owned(),
            state,
            last_message: Some(last_message),
            last_attachment: None,
            last_sticker: None,
            unread_count,
        }
    }

    #[test]
    fn a_muted_peer_never_counts() {
        use dyapp_client::interactions::{Interaction, Kind};

        let dir = std::env::temp_dir().join(format!("dyapp-notify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (me, peer) = (Identity::generate(), Identity::generate());
        let (me_id, peer_id) = (me.peer_id(), peer.peer_id());
        let like = |store: &mut dyapp_client::store::Store, sender: &str, target: &str, n: u8| {
            let kind = Kind::Like;
            let (event_id, hlc) = ([n; 16], u64::from(n) << 16);
            let interaction = Interaction {
                kind,
                event_id,
                hlc,
            };
            store
                .apply_interaction(sender, target, &[n; 32], interaction)
                .unwrap();
        };
        let counts = || {
            let (rows, likes) = waiting(&dir, &me).unwrap();
            (rows.len(), likes)
        };
        let mut store = crate::chats::open(&dir, &me).unwrap();
        like(&mut store, &peer_id, &me_id, 1);
        assert_eq!(counts(), (0, 1));
        store.set_muted(&peer_id, true).unwrap();
        assert_eq!(counts(), (0, 0));
        like(&mut store, &me_id, &peer_id, 2);
        assert_eq!(counts(), (0, 0));
        store.set_muted(&peer_id, false).unwrap();
        assert_eq!(counts(), (1, 0));
    }

    fn shown(mode: NoticeContent, state: Conversation) -> (String, String) {
        let before = [row("old", Conversation::Open, "seen", 1)];
        let after = [
            row("new", state, "<b>hi</b> & bye\nnext", 1),
            row("old", Conversation::Open, "seen", 1),
        ];
        let (name, body) = content(mode, &before, &after, |peer| format!("name of {peer}"));
        (name, body.render("Sticker"))
    }

    #[test]
    fn the_default_notice_carries_neither_name_nor_text() {
        let nothing = (String::new(), String::new());
        assert_eq!(shown(NoticeContent::default(), Conversation::Open), nothing);
    }

    #[test]
    fn the_setting_adds_the_name_and_the_escaped_text_of_the_newest_arrival() {
        let name = "name of new".to_owned();
        assert_eq!(
            shown(NoticeContent::Name, Conversation::Open),
            (name.clone(), String::new())
        );
        assert_eq!(
            shown(NoticeContent::Text, Conversation::Open),
            (name, "&lt;b&gt;hi&lt;/b&gt; &amp; bye next".to_owned())
        );
    }

    #[test]
    fn a_request_stays_fixed_whatever_the_setting() {
        let request = Conversation::RequestReceived([1; 16]);
        let nothing = (String::new(), String::new());
        assert_eq!(shown(NoticeContent::Text, request), nothing);
    }

    #[test]
    fn a_body_is_bounded_and_holds_only_an_unread_text() {
        let mut long = row("a", Conversation::Open, &"x".repeat(BODY_CHARS * 2), 1);
        assert_eq!(body(&long).render("Sticker").chars().count(), BODY_CHARS);
        long.last_message.as_mut().unwrap().unread = false;
        assert_eq!(body(&long).render("Sticker"), "");
    }

    #[test]
    fn sticker_preview_is_localized_and_never_reveals_older_text_or_pack_metadata() {
        let mut row = row("peer", Conversation::Open, "older private text", 2);
        row.last_sticker = Some(dyapp_client::store::messages::ChatSticker {
            key: MessageKey {
                group: vec![1],
                author: "peer".into(),
                id: [2; 16],
            },
            sticker: dyapp_client::chat::sticker::Sticker {
                id: [2; 16],
                hlc: 2,
                pack_id: [3; 32],
                sha256: [4; 32],
                emoji: "😺".into(),
                pack_name: "private pack name".into(),
                attachment: None,
            },
            unread: true,
            delivery: None,
        });
        let shown = |mode, row: &ChatRow, label| {
            let (name, preview) =
                content(mode, &[], std::slice::from_ref(row), |_| "sender".into());
            (name, preview.render(label))
        };
        assert_eq!(
            shown(NoticeContent::default(), &row, "Sticker"),
            (String::new(), String::new())
        );
        assert_eq!(
            shown(NoticeContent::Name, &row, "Sticker"),
            ("sender".to_owned(), String::new())
        );
        assert_eq!(
            shown(NoticeContent::Text, &row, "Sticker"),
            ("sender".to_owned(), "😺 Sticker".to_owned())
        );
        assert_eq!(
            shown(NoticeContent::Text, &row, "Стикер"),
            ("sender".to_owned(), "😺 Стикер".to_owned())
        );
        row.last_sticker.as_mut().unwrap().sticker.emoji = "<".into();
        assert_eq!(body(&row).render("Sticker"), "&lt; Sticker");
        row.last_sticker.as_mut().unwrap().unread = false;
        assert_eq!(body(&row).render("Sticker"), "");
        row.last_sticker.as_mut().unwrap().unread = true;
        row.state = Conversation::Closed;
        assert_eq!(
            shown(NoticeContent::Text, &row, "Sticker"),
            (String::new(), String::new())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_notice_offers_the_click_and_no_hints() {
        let actions = ["default", "Open"];
        let (application, replaces, icon, summary, text, actions, hints, _) =
            notice("New message", "", &actions, 7);
        assert_eq!((application, replaces, icon), ("DYApp", 7, ""));
        assert_eq!((summary, text), ("New message", ""));
        assert_eq!(actions, ["default", "Open"]);
        assert!(hints.is_empty());
    }
}
