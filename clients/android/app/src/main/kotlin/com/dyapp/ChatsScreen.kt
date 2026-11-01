package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Badge
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import java.text.DateFormat
import java.util.Date

// As the desktop's mailbox poll.
private const val RECEIVE_INTERVAL_MS = 60_000L

/**
 * This identity's chat rows, read locally first and again after each mailbox receive while the
 * activity is started. The shell owns them because the tab badge shows on every section.
 */
@Composable
internal fun rememberChatRows(opened: Opened.Ready): List<ChatRow> {
    // Keyed so that a switch never shows the previous identity's chats, even for a frame.
    val rows = remember(opened.slot.id) { mutableStateOf(emptyList<ChatRow>()) }
    val owner = checkNotNull(LocalView.current.findViewTreeLifecycleOwner()) { "Compose needs a lifecycle" }
    LaunchedEffect(rows, owner) {
        owner.repeatOnLifecycle(Lifecycle.State.STARTED) {
            // On IO the delay is real time, so Compose tests do not fast-forward this endless loop.
            withContext(Dispatchers.IO) { pollChats(opened) { rows.value = it } }
        }
    }
    return rows.value
}

// shortcut: a poll in flight can make an identity removal fail after the rename, leaving its
// hidden folder as a crash does; removal runs on the UI thread, so it cannot wait for a receive.
// Upgrade when removal moves off the UI thread: take the chats lock there.
private suspend fun pollChats(opened: Opened.Ready, show: (List<ChatRow>) -> Unit) {
    // Failures stay quiet and keep the last rows: no network and an unpublished profile are normal.
    fun read() = runCatching { RustBridge.withChats(opened) { it.chatRows() } }.onSuccess(show)
    read()
    while (true) {
        runCatching { RustBridge.withChats(opened) { it.receive() } }
        read()
        delay(RECEIVE_INTERVAL_MS)
    }
}

/** The tab badge: unread messages plus message requests; a request's own unread text counts once. */
internal fun chatsBadge(rows: List<ChatRow>): ULong = rows.sumOf {
    if (it.state == ChatState.REQUEST_RECEIVED) maxOf(it.unreadCount, 1uL) else it.unreadCount
}

/** The Chats section: this identity's conversations and message requests. */
@Composable
internal fun ChatsScreen(rows: List<ChatRow>) {
    Column(Modifier.fillMaxSize().testTag("chats")) {
        if (rows.isEmpty()) {
            Text(stringResource(R.string.chats_empty), Modifier.padding(16.dp))
        } else {
            LazyColumn(Modifier.weight(1f).testTag("chat-list")) {
                itemsIndexed(rows, key = { _, row -> row.peerId }) { index, row ->
                    ChatListRow(row, Modifier.testTag("chat-${index + 1}"))
                }
            }
        }
        Caption(stringResource(R.string.chats_identity), Modifier.padding(16.dp))
    }
}

@Composable
private fun ChatListRow(row: ChatRow, modifier: Modifier) {
    Row(
        modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
        Arrangement.spacedBy(12.dp),
        Alignment.CenterVertically,
    ) {
        Avatar(row.peerId)
        Column(Modifier.weight(1f)) {
            Text(shortPeerId(row.peerId), style = MaterialTheme.typography.titleSmall)
            if (row.state == ChatState.REQUEST_RECEIVED) {
                Text(stringResource(R.string.chats_request), style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.primary)
            }
            Text(chatPreview(row.lastMessage), maxLines = 1, overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            chatTime(row.lastMessage?.sentAtMs)?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
            if (row.unreadCount > 0uL) UnreadBadge(row.unreadCount, Modifier)
        }
    }
}

/** A count badge that reads as the number of unread messages. */
@Composable
internal fun UnreadBadge(count: ULong, modifier: Modifier) {
    val description = stringResource(R.string.chats_unread, count.toString())
    Badge(modifier.semantics { contentDescription = description }) { Text(count.toString()) }
}

@Composable
private fun chatPreview(message: ChatMessage?): String = when {
    message == null -> stringResource(R.string.chats_no_messages)
    message.deleted -> stringResource(R.string.chats_deleted)
    message.forwarded -> stringResource(R.string.chats_forwarded, message.text.orEmpty())
    else -> message.text.orEmpty()
}

@Composable
private fun chatTime(sentAtMs: ULong?): String? {
    val locale = LocalConfiguration.current.locales[0]
    return sentAtMs?.let {
        DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT, locale).format(Date(it.toLong()))
    }
}
