//! One notice for mail that arrived while the window has no focus. By default it holds only a
//! fixed text in the UI language, because a desktop can show it on the lock screen; the identity's
//! settings can add the sender's name and the message.

use std::path::Path;
#[cfg(unix)]
use std::sync::atomic::{AtomicU32, Ordering};

use dyapp_client::store::{ChatRow, Conversation, NoticeContent};
use dyapp_identity::Identity;
use tauri::{AppHandle, Manager};

/// The most of a message a notice shows; the rest waits in the app.
const BODY_CHARS: usize = 200;

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

/// The newest row with more unread items than before.
fn newest<'a>(before: &[ChatRow], after: &'a [ChatRow]) -> Option<&'a ChatRow> {
    after.iter().find(|row| {
        let old = before.iter().find(|old| old.peer_id == row.peer_id);
        row.unread_count > old.map_or(0, |old| old.unread_count)
    })
}

/// UI labels are resolved on the detached send thread, after the session lock is released.
#[derive(Default)]
enum Preview {
    #[default]
    Empty,
    Text(String),
    Sticker(String),
}
impl Preview {
    // A server may interpret markup. A server without body-markup shows these entities.
    fn render(self, sticker_label: &str) -> String {
        let text = match self {
            Self::Empty => String::new(),
            Self::Text(text) => text,
            Self::Sticker(emoji) => format!("{emoji} {sticker_label}"),
        };
        text.chars()
            .take(BODY_CHARS)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}
fn body(row: &ChatRow) -> Preview {
    // Core retains older text behind the newest media marker; never expose that stale text.
    if let Some(item) = &row.last_sticker {
        return if item.unread {
            Preview::Sticker(item.sticker.emoji.clone())
        } else {
            Preview::Empty
        };
    }
    let last = row.last_attachment.is_none();
    let message = row.last_message.as_ref().filter(|m| last && m.unread);
    let text = message.and_then(|m| m.current.text.as_deref());
    let text = text.and_then(|text| std::str::from_utf8(text).ok());
    text.map_or(Preview::Empty, |text| {
        Preview::Text(text.chars().take(BODY_CHARS).collect())
    })
}

/// The name and text the setting allows; empty ones leave the fixed text alone. Only an open chat
/// adds them: a request or a like would put a stranger's words on the lock screen unasked.
fn content(
    mode: NoticeContent,
    before: &[ChatRow],
    after: &[ChatRow],
    name: impl FnOnce(&str) -> String,
) -> (String, Preview) {
    let row = newest(before, after).filter(|row| row.state == Conversation::Open);
    match (mode, row) {
        (NoticeContent::Name, Some(row)) => (name(&row.peer_id), Preview::Empty),
        (NoticeContent::Text, Some(row)) => (name(&row.peer_id), body(row)),
        _ => Default::default(),
    }
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
        std::thread::spawn(move || {
            let window = app.get_webview_window("main");
            if !window.is_some_and(|window| window.is_focused().unwrap_or(false)) {
                if let Some((open, fixed, sticker)) = crate::tray::notice_labels(&app) {
                    let body = body.render(&sticker);
                    send(if name.is_empty() { &fixed } else { &name }, &body, &open);
                }
            }
        });
    }
    result
}

#[cfg(unix)]
const SERVER: &str = "org.freedesktop.Notifications";
#[cfg(unix)]
const PATH: &str = "/org/freedesktop/Notifications";
/// The notice's own connection: a server can tie a notice to it, and send it the click alone.
#[cfg(unix)]
static BUS: std::sync::OnceLock<zbus::Connection> = std::sync::OnceLock::new();
#[cfg(unix)]
static LAST: AtomicU32 = AtomicU32::new(0);

/// Opens the connection for notices and shows the window when the last one is clicked. A desktop
/// without a session bus shows neither.
#[cfg(unix)]
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

#[cfg(not(unix))]
pub fn watch(_app: AppHandle) {}

#[cfg(unix)]
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
#[cfg(unix)]
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
#[cfg(unix)]
fn send(summary: &str, body: &str, open: &str) {
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

// ponytail: Windows shows no notice yet; add the toast API when that client ships.
#[cfg(not(unix))]
fn send(_summary: &str, _body: &str, _open: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_client::store::messages::{ChatMessage, CurrentMessage, MessageKey};

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

    #[cfg(unix)]
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
