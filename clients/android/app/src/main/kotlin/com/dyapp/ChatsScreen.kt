package com.dyapp

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Badge
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import uniffi.dyapp.ContactNameView
import uniffi.dyapp.FoundProfile
import java.text.DateFormat
import java.util.Date

/** The Chats section; the Liked you profiles come from the network, so only an open list fetches them. */
@Composable
internal fun ChatsScreen(feed: ChatFeed, onOpen: (String) -> Unit, onProfile: (FoundProfile) -> Unit) {
    val lists = feed.lists
    val liked = feed.liked
    // Through the searcher when there is one, so the fetched profiles name and show those peers.
    val photos = LocalPeerPhotos.current
    LaunchedEffect(lists.likedOpen, liked) {
        if (lists.likedOpen && liked.isNotEmpty()) {
            lists.likedProfiles = withContext(Dispatchers.IO) {
                runCatching { photos?.profiles(liked) ?: feed.profiles(liked) }
            }
        }
    }
    FetchPeerProfiles(feed.rows.map { it.peerId }.sorted())
    ChatsContent(feed.rows, liked, lists, onOpen, onProfile)
}

/** Liked you and message requests with counts from local data, then the conversations. */
@Composable
@Suppress("LongParameterList") // A stateless screen: its tests pass each list and route.
internal fun ChatsContent(
    rows: List<ChatRow>,
    liked: List<String>,
    lists: ChatsLists,
    onOpen: (String) -> Unit,
    onProfile: (FoundProfile) -> Unit,
) {
    val (requests, conversations) = rows.partition { it.state == ChatState.REQUEST_RECEIVED }
    val names = rememberContactNames(rows.map { it.peerId }.sorted())
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).testTag("chats")) {
        lists.blocked?.let { BlockedNote(it) }
        ExpandableRow("chats-liked", R.string.chats_liked, R.string.chats_liked_text, liked.size, lists.likedOpen) {
            lists.likedOpen = !lists.likedOpen
        }
        if (lists.likedOpen) LikedList(liked, lists.likedProfiles, onProfile)
        ExpandableRow("chats-requests", R.string.chats_requests, R.string.chats_requests_text, requests.size,
            lists.requestsOpen) { lists.requestsOpen = !lists.requestsOpen }
        if (lists.requestsOpen) RequestList(requests, names, onOpen)
        Text(stringResource(R.string.chats_conversations), Modifier.padding(16.dp),
            style = MaterialTheme.typography.titleMedium)
        if (conversations.isEmpty()) {
            Text(stringResource(R.string.chats_empty), Modifier.padding(horizontal = 16.dp))
        } else {
            // Not lazy, as it scrolls with the lists above; all rows are in memory anyway.
            Column(Modifier.testTag("chat-list")) {
                for ((index, row) in conversations.withIndex()) {
                    ChatListRow(row, names[row.peerId],
                        Modifier.testTag("chat-${index + 1}").clickable { onOpen(row.peerId) })
                }
            }
        }
        Caption(stringResource(R.string.chats_identity), Modifier.padding(16.dp))
    }
}

@Composable
@Suppress("LongParameterList") // The two rows differ in each of these.
private fun ExpandableRow(tag: String, title: Int, text: Int, count: Int, open: Boolean, toggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().testTag(tag).clickable(role = Role.Button, onClick = toggle)
            .semantics { if (open) collapse { toggle(); true } else expand { toggle(); true } }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        Arrangement.spacedBy(12.dp),
        Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), style = MaterialTheme.typography.titleSmall)
            Caption(stringResource(text))
        }
        Badge { Text(count.toString(), Modifier.testTag("$tag-count")) }
    }
}

// Closing and opening the list fetches again, so a failure needs no retry button of its own.
@Composable
private fun LikedList(liked: List<String>, fetch: Result<List<FoundProfile>>?, open: (FoundProfile) -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("chats-liked-list"),
        Arrangement.spacedBy(12.dp)) {
        val failure = fetch?.exceptionOrNull()
        when {
            liked.isEmpty() -> {
                Caption(stringResource(R.string.chats_none))
            }
            fetch == null -> {
                Caption(stringResource(R.string.view_loading),
                    Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            }
            failure != null -> {
                val status = profileLoadStatus(failure)
                ErrorNotice(stringResource(status.text), "chats-liked-status", status.details.orEmpty(),
                    "chats-liked-details", "copy-chats-liked-details")
            }
            else -> {
                LikedProfiles(liked, fetch.getOrDefault(emptyList()), open)
            }
        }
    }
}

// The fetch can be older than the IDs, so a peer who left the list leaves at once.
@Composable
private fun LikedProfiles(liked: List<String>, found: List<FoundProfile>, open: (FoundProfile) -> Unit) {
    val shown = found.filter { it.peerId in liked }
    FetchCardPhotos(shown)
    for ((index, profile) in shown.withIndex()) {
        SearchResultRow(profile, "liked-${index + 1}") { open(profile) }
    }
    if (shown.size < liked.size) Caption(stringResource(R.string.chats_liked_missing))
}

@Composable
private fun RequestList(requests: List<ChatRow>, names: Map<String, ContactNameView>, onOpen: (String) -> Unit) {
    Column(Modifier.testTag("chats-requests-list")) {
        if (requests.isEmpty()) {
            Caption(stringResource(R.string.chats_no_requests), Modifier.padding(horizontal = 16.dp))
        }
        for ((index, row) in requests.withIndex()) {
            ChatListRow(row, names[row.peerId],
                Modifier.testTag("request-${index + 1}").clickable { onOpen(row.peerId) })
        }
    }
}

@Composable
private fun ChatListRow(row: ChatRow, name: ContactNameView?, modifier: Modifier) {
    Row(
        modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
        Arrangement.spacedBy(12.dp),
        Alignment.CenterVertically,
    ) {
        PeerAvatar(row.peerId, name)
        Column(Modifier.weight(1f)) {
            Text(contactLabel(row.peerId, name), style = MaterialTheme.typography.titleSmall)
            Text(chatPreview(row.lastMessage), maxLines = 1, overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            chatTime(row.lastMessage?.sentAtMs)?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
            if (row.unreadCount > 0uL) UnreadBadge(row.unreadCount, Modifier)
            if (row.muted) Caption(stringResource(R.string.chats_muted), Modifier.testTag("chat-muted"))
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
internal fun chatPreview(message: ChatMessage?): String = when {
    message == null -> stringResource(R.string.chats_no_messages)
    message.deleted -> stringResource(R.string.chats_deleted)
    message.forwarded -> stringResource(R.string.chats_forwarded, message.text.orEmpty())
    else -> message.text.orEmpty()
}

@Composable
internal fun chatTime(sentAtMs: ULong?): String? {
    val locale = LocalConfiguration.current.locales[0]
    return sentAtMs?.let {
        DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT, locale).format(Date(it.toLong()))
    }
}
