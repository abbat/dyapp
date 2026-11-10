package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatBlock
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ChatState
import uniffi.dyapp.FoundProfile

// As the desktop's mailbox poll.
private const val RECEIVE_INTERVAL_MS = 60_000L

/** The section's expandable lists; the feed keeps them while a chat or a profile covers the section. */
internal class ChatsLists {
    var likedOpen by mutableStateOf(false)
    var requestsOpen by mutableStateOf(false)

    /** The last fetch of the Liked you profiles; null until the first one ends. */
    var likedProfiles by mutableStateOf<Result<List<FoundProfile>>?>(null)

    /** The last block from a chat, said on the section until another chat opens. */
    var blocked by mutableStateOf<ChatBlock?>(null)
}

/**
 * This identity's chat rows; the shell owns them because the tab badge shows on every section.
 * The blocking calls belong on IO.
 */
internal class ChatFeed(internal val opened: Opened.Ready) {
    var rows by mutableStateOf(emptyList<ChatRow>())
        private set

    /** The Liked you peers, read locally with the rows. */
    var liked by mutableStateOf(emptyList<String>())
        private set

    val lists = ChatsLists()
    private val photos = ChatPhotoFeed(opened, ::read)

    /** Counts the reads, so an open chat reloads for a change the rows do not show. */
    var version by mutableIntStateOf(0)
        private set

    // Failures stay quiet and keep the last rows: no network and an unpublished profile are normal.
    internal fun read() {
        runCatching {
            RustBridge.withChats(opened) {
                rows = it.chatRows()
                version += 1
                liked = it.likedYou()
            }
        }
    }

    fun history(peer: String) = RustBridge.withChats(opened) { it.chatHistory(peer) }

    /** The core supplies order; never merge photos by wall time in Kotlin. */
    fun thread(peer: String) = RustBridge.withChats(opened) {
        ChatThreadSnapshot(
            it.chatHistory(peer),
            it.chatItems(peer),
            { photo -> photos.original(peer, photo) },
            { photos.auto(peer) },
            { photo -> photos.send(peer, photo) }
        )
    }.also { markRead(peer) }

    /** Core commitment results stay distinct from pre-commit failures and display status. */
    fun send(peer: String, text: String) = change { it.submitText(peer, text) }

    fun edit(peer: String, message: ChatMessage, text: String) =
        change { it.submitEditText(peer, message.groupId, message.id, text) }

    fun delete(peer: String, message: ChatMessage) =
        change { it.submitDeleteText(peer, message.groupId, message.id) }

    private fun change(action: (ChatSession) -> uniffi.dyapp.ChatSubmission) =
        runCatching { RustBridge.withChats(opened, action) }.also { read() }

    /**
     * Marks [peer]'s texts read; only a change reads the rows, so a failure cannot loop through an
     * open chat's reload. Then it sends the read receipts the shown chat owes, none while the setting
     * is off and without the network when none is due; a failed send is quiet and waits for the next read.
     */
    fun markRead(peer: String) {
        val marked = runCatching { RustBridge.withChats(opened) { it.markRead(peer) } }.getOrDefault(0u)
        runCatching { RustBridge.withChats(opened) { it.sendReads(peer) } }
        if (marked > 0u) read()
    }
}

// shortcut: a poll in flight can make an identity removal fail after the rename, leaving its
// hidden folder as a crash does; removal runs on the UI thread, so it cannot wait for a receive.
// Upgrade when removal moves off the UI thread: take the chats lock there.
private suspend fun ChatFeed.poll() {
    read()
    while (true) {
        runCatching { RustBridge.withChats(opened) { it.receive() } }
        read()
        delay(RECEIVE_INTERVAL_MS)
    }
}

/** The rows read locally first and again after each mailbox receive while the activity is started. */
@Composable
internal fun rememberChatFeed(opened: Opened.Ready): ChatFeed {
    // Keyed so that a switch never shows the previous identity's chats, even for a frame.
    val feed = remember(opened.slot.id) { ChatFeed(opened) }
    val owner = checkNotNull(LocalView.current.findViewTreeLifecycleOwner()) { "Compose needs a lifecycle" }
    LaunchedEffect(feed, owner) {
        owner.repeatOnLifecycle(Lifecycle.State.STARTED) {
            // On IO the delay is real time, so Compose tests do not fast-forward this endless loop.
            withContext(Dispatchers.IO) { feed.poll() }
        }
    }
    return feed
}

/** The tab badge: unread messages plus message requests; a request's own unread text counts once. */
internal fun chatsBadge(rows: List<ChatRow>): ULong = rows.sumOf {
    if (it.state == ChatState.REQUEST_RECEIVED) maxOf(it.unreadCount, 1uL) else it.unreadCount
}
