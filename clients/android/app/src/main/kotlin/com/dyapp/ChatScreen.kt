package com.dyapp

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatDelivery
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

// As the core's limit and the desktop composer.
private const val MAX_TEXT_BYTES = 65_536
private const val META_SEPARATOR = " | "
private const val COMPOSER_MAX_LINES = 5

/** A line under the thread: a failure carries [details] and keeps the draft; a queued send has none. */
internal class ChatStatus(val text: Int, val details: String?)

/** The composer: [draft] survives recreation, a send in flight and its last outcome do not. */
internal class ChatForm(val draft: MutableState<String>) {
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)
}

/** A shown chat takes text unless the first message waits for a reply; null is still loading. */
internal fun chatWritable(state: ChatState?) = state != null && state != ChatState.REQUEST_SENT

internal fun chatTooLong(text: String) = text.toByteArray().size > MAX_TEXT_BYTES

internal fun chatSendable(state: ChatState?, text: String) =
    chatWritable(state) && text.isNotBlank() && !chatTooLong(text)

/** Whether a failed send still stored its text: another copy from the same draft must not go out. */
internal fun stagedNew(before: List<ChatMessage>, after: List<ChatMessage>, me: String): Boolean {
    val known = before.map { it.id.toList() }.toSet()
    return after.any { it.author == me && it.id.toList() !in known }
}

internal fun sendStatus(error: Throwable, staged: Boolean): ChatStatus {
    val details = error.toString()
    return when {
        staged -> ChatStatus(R.string.chat_queued, null)
        "notConnected" in details -> ChatStatus(R.string.search_not_connected, details)
        "unpublished" in details -> ChatStatus(R.string.chat_unpublished, details)
        else -> ChatStatus(R.string.chat_send_failed, details)
    }
}

/**
 * One conversation without the app bars: reloaded whenever [feed] reads the rows, so a received
 * text or a delivery update shows here too. Back returns to the Chats section.
 */
@Composable
@Suppress("LongParameterList") // The header opens the profile page and back leaves the chat.
internal fun ChatScreen(
    opened: Opened.Ready,
    peer: String,
    feed: ChatFeed,
    onProfile: () -> Unit,
    onBack: () -> Unit,
) {
    var history by remember { mutableStateOf<ChatHistory?>(null) }
    val draft = rememberSaveable { mutableStateOf("") }
    val form = remember { ChatForm(draft) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(feed.version) {
        withContext(Dispatchers.IO) { runCatching { feed.history(peer) } }
            .onSuccess { shown ->
                if (history == null) form.status = null
                history = shown
            }
            .onFailure {
                if (history == null) form.status = ChatStatus(R.string.chat_load_failed, it.toString())
            }
    }
    BackHandler(onBack = onBack)
    val send: () -> Unit = {
        val before = history?.messages.orEmpty()
        val text = draft.value
        form.sending = true
        form.status = null
        scope.launch {
            val status = withContext(Dispatchers.IO) { feed.send(peer, text, before) }
            form.sending = false
            form.status = status
            if (status?.details == null) draft.value = ""
        }
    }
    ChatContent(peer, opened.peerId, history, form, send, onProfile, onBack)
}

/** The chat as shown: the state's notices, the thread and the composer. */
@Composable
@Suppress("LongParameterList") // Stateless so that tests drive every chat state without a network.
internal fun ChatContent(
    peer: String,
    me: String,
    history: ChatHistory?,
    form: ChatForm,
    onSend: () -> Unit,
    onProfile: () -> Unit,
    onBack: () -> Unit,
) {
    val state = history?.state
    Column(Modifier.fillMaxSize().safeDrawingPadding().testTag("chat")) {
        ChatBackBar(peer, onProfile, onBack)
        ChatStateNotice(state)
        ChatThread(me, history?.messages.orEmpty(), Modifier.weight(1f))
        Column(Modifier.padding(horizontal = 16.dp), Arrangement.spacedBy(8.dp)) {
            if (state == ChatState.REQUEST_SENT) Caption(stringResource(R.string.chat_first_wait))
            ChatStatusLine(form)
            if (chatTooLong(form.draft.value)) Caption(stringResource(R.string.chat_too_long))
        }
        ChatComposer(state, form, onSend)
    }
}

@Composable
private fun ChatBackBar(peer: String, onProfile: () -> Unit, onBack: () -> Unit) {
    Row(Modifier.padding(8.dp), Arrangement.spacedBy(8.dp), Alignment.CenterVertically) {
        IconButton(onClick = onBack, Modifier.testTag("chat-back")) {
            Icon(painterResource(R.drawable.ic_back), stringResource(R.string.nav_chats))
        }
        Row(Modifier.testTag("chat-profile").clickable(onClick = onProfile), Arrangement.spacedBy(8.dp),
            Alignment.CenterVertically) {
            Avatar(peer)
            Text(shortPeerId(peer), Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium)
        }
    }
}

// The one-message rule matters before that message too, so it also shows in a new conversation.
@Composable
private fun ChatStateNotice(state: ChatState?) {
    val modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp)
    when (state) {
        ChatState.NONE, ChatState.REQUEST_SENT ->
            Caption(stringResource(R.string.chat_first_notice), modifier.testTag("chat-first-notice"))
        ChatState.REQUEST_RECEIVED -> Surface(
            modifier.fillMaxWidth(), RoundedCornerShape(12.dp), MaterialTheme.colorScheme.tertiaryContainer
        ) {
            Text(stringResource(R.string.chat_request_notice), Modifier.padding(12.dp).testTag("chat-request-notice"))
        }
        else -> Unit
    }
}

@Composable
private fun ChatThread(me: String, messages: List<ChatMessage>, modifier: Modifier) {
    val list = rememberLazyListState()
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) list.scrollToItem(messages.lastIndex) }
    LazyColumn(
        modifier.fillMaxWidth().testTag("chat-thread"),
        state = list,
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(messages) { ChatBubble(it, it.author == me) }
    }
}

@Composable
private fun ChatBubble(message: ChatMessage, own: Boolean) {
    Box(Modifier.fillMaxWidth(), if (own) Alignment.CenterEnd else Alignment.CenterStart) {
        Surface(
            Modifier.padding(start = if (own) 48.dp else 0.dp, end = if (own) 0.dp else 48.dp),
            RoundedCornerShape(12.dp),
            if (own) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer,
        ) {
            Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                Text(chatPreview(message))
                chatMeta(message).takeIf { it.isNotEmpty() }?.let { Caption(it) }
            }
        }
    }
}

@Composable
private fun chatMeta(message: ChatMessage): String = listOfNotNull(
    chatTime(message.sentAtMs),
    stringResource(R.string.chat_edited).takeIf { message.edited && !message.deleted },
    message.delivery?.takeIf { !message.deleted }?.let { stringResource(deliveryText(it)) },
).joinToString(META_SEPARATOR)

// Node acceptance reads as Sent: only a device of the peer makes it Delivered.
private fun deliveryText(delivery: ChatDelivery) = when (delivery) {
    ChatDelivery.PENDING -> R.string.chat_pending
    ChatDelivery.NODE_ACCEPTED -> R.string.chat_sent
    ChatDelivery.DEVICE_RECEIVED -> R.string.chat_delivered
}

@Composable
private fun ChatStatusLine(form: ChatForm) {
    val status = form.status
    val line = Modifier.testTag("chat-status").semantics { liveRegion = LiveRegionMode.Polite }
    when {
        form.sending -> Caption(stringResource(R.string.chat_sending), line)
        status == null -> Unit
        status.details == null -> Caption(stringResource(status.text), line)
        else -> ErrorNotice(stringResource(status.text), "chat-status", status.details, "chat-details",
            "copy-chat-details")
    }
}

@Composable
private fun ChatComposer(state: ChatState?, form: ChatForm, onSend: () -> Unit) {
    val text = form.draft.value
    Row(Modifier.fillMaxWidth().padding(16.dp), Arrangement.spacedBy(8.dp), Alignment.CenterVertically) {
        // Read-only rather than disabled while sending, so the keyboard stays for the next text.
        OutlinedTextField(
            text,
            { form.draft.value = it },
            Modifier.weight(1f).testTag("chat-input"),
            enabled = chatWritable(state),
            readOnly = form.sending,
            placeholder = { Text(stringResource(R.string.chat_placeholder)) },
            maxLines = COMPOSER_MAX_LINES,
        )
        IconButton(
            onClick = onSend,
            Modifier.testTag("chat-send"),
            enabled = !form.sending && chatSendable(state, text),
        ) {
            Icon(painterResource(R.drawable.ic_send), stringResource(R.string.chat_send))
        }
    }
}
