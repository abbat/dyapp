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
import uniffi.dyapp.ChatLikes
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
@Suppress("TooManyFunctions") // One per chat call the screens make through the bridge.
internal class ChatFeed(private val opened: Opened.Ready) {
    var rows by mutableStateOf(emptyList<ChatRow>())
        private set

    /** The Liked you peers, read locally with the rows. */
    var liked by mutableStateOf(emptyList<String>())
        private set

    val lists = ChatsLists()

    /** Counts the reads, so an open chat reloads for a change the rows do not show. */
    var version by mutableIntStateOf(0)
        private set

    // Failures stay quiet and keep the last rows: no network and an unpublished profile are normal.
    private fun read() {
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
        ChatThreadSnapshot(it.chatHistory(peer), it.chatItems(peer),
            { photo -> photoOriginal(peer, photo) }, { photoAuto(peer) })
    }.also { markRead(peer) }

    fun photoOriginal(peer: String, photo: uniffi.dyapp.ChatPhoto) = RustBridge.withChats(opened) {
        it.chatPhotoOriginal(peer, photo.groupId, photo.author, photo.id)
    }

    fun photoAuto(peer: String) = RustBridge.withChats(opened) { it.chatPhotoAuto(peer) }

    fun likes(peer: String) = RustBridge.withChats(opened) { it.likes(peer) }

    /** The verified profiles of [peers]; a peer no node returns is left out. */
    fun profiles(peers: List<String>) = RustBridge.withChats(opened) { it.profiles(peers) }

    /** Likes [peer] or takes the like back, then reads the rows, which also rereads the page's likes. */
    fun like(peer: String, liked: Boolean): Result<ChatLikes> {
        val result = runCatching { RustBridge.withChats(opened) { it.like(peer, liked) } }
        read()
        return result
    }

    /** Passes on [peer] or undoes that, here only and unseen by the peer, then reads the rows. */
    fun pass(peer: String, passed: Boolean): Result<ChatLikes> =
        runCatching { RustBridge.withChats(opened) { it.setPassed(peer, passed) } }.also { read() }

    /** Sends [text] and reads the rows; the outcome to show, null when a node took it. */
    fun send(peer: String, text: String, before: List<ChatMessage>): ChatStatus? {
        val error = runCatching { RustBridge.withChats(opened) { it.sendText(peer, text) } }.exceptionOrNull()
        read()
        return error?.let { failed ->
            val after = runCatching { history(peer).messages }.getOrDefault(before)
            sendStatus(failed, stagedNew(before, after, opened.peerId))
        }
    }

    /** Shares a core-formatted profile link, only into a still-open conversation. Call on IO. */
    fun shareProfile(peer: String, sharedPeer: String): ChatStatus? {
        val text = requireNotNull(uniffi.dyapp.profileLinkText(sharedPeer)) { "invalid profile peer" }
        val before = history(peer)
        require(before.state == ChatState.OPEN && peer != sharedPeer) { "not an eligible chat" }
        return send(peer, text, before.messages)
    }

    /** Replaces the text of the own [message]; the outcome to show, null when a node took it. */
    fun edit(peer: String, message: ChatMessage, text: String): ChatStatus? {
        val error = change { it.editText(peer, message.groupId, message.id, text) } ?: return null
        val stored = shown(peer, message)?.let { it.edited && !it.deleted && it.text == text } == true
        return sendStatus(error, stored, R.string.chat_edit_queued, R.string.chat_edit_failed)
    }

    /** Deletes the own [message] here and asks the peer's app to delete it too. */
    fun delete(peer: String, message: ChatMessage): ChatStatus? {
        val error = change { it.deleteText(peer, message.groupId, message.id) } ?: return null
        val stored = shown(peer, message)?.deleted == true
        return sendStatus(error, stored, R.string.chat_delete_queued, R.string.chat_delete_failed)
    }

    // A failed delivery can follow a stored change, as for a send; the rows are read either way.
    private fun change(action: (ChatSession) -> Unit): Throwable? =
        runCatching { RustBridge.withChats(opened, action) }.exceptionOrNull().also { read() }

    private fun shown(peer: String, message: ChatMessage): ChatMessage? =
        runCatching { history(peer).messages.byKey(messageKey(message)) }.getOrNull()

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

    /** Clears the chat with [peer] on this device only, then reads the rows. */
    fun clear(peer: String): Result<UInt> =
        runCatching { RustBridge.withChats(opened) { it.clearChat(peer) } }.also { read() }

    /** Mutes [peer] or ends that; only the new-mail notice changes. */
    fun mute(peer: String, muted: Boolean): Result<Unit> =
        runCatching { RustBridge.withChats(opened) { it.setMuted(peer, muted) } }.also { read() }

    /** Names [peer] on this device only; an empty [name] removes the name. */
    fun rename(peer: String, name: String): Result<Unit> =
        runCatching { RustBridge.withChats(opened) { it.setContactName(peer, name) } }

    /** Blocks [peer]; the rows read after it leave the peer out, and [lists] keeps the outcome. */
    fun block(peer: String): Result<ChatBlock> =
        runCatching { RustBridge.withChats(opened) { it.block(peer) } }
            .onSuccess { lists.blocked = it }
            .also { read() }

    // shortcut: a poll in flight can make an identity removal fail after the rename, leaving its
    // hidden folder as a crash does; removal runs on the UI thread, so it cannot wait for a receive.
    // Upgrade when removal moves off the UI thread: take the chats lock there.
    suspend fun poll() {
        read()
        while (true) {
            runCatching { RustBridge.withChats(opened) { it.receive() } }
            read()
            delay(RECEIVE_INTERVAL_MS)
        }
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
