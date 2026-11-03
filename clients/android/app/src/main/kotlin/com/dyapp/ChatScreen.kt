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
import androidx.compose.material3.OutlinedButton
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
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatDelivery
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

private const val STATUS_TAG = "chat-status"
private const val META_SEPARATOR = " | "
private const val COMPOSER_MAX_LINES = 5

/** A line under the thread: a failure carries [details] and keeps the draft; a queued send has none. */
internal class ChatStatus(val text: Int, val details: String?)

/**
 * The composer and the menus' state: [draft], the open [dialog], the [editing] and [deleting] targets as
 * [messageKey]s and the [held] draft survive recreation; a change in flight, its last outcome and the
 * cleared note do not.
 */
internal class ChatForm(
    val draft: MutableState<String>,
    val dialog: MutableState<ChatDialog?> = mutableStateOf(null),
    val editing: MutableState<String?> = mutableStateOf(null),
    val deleting: MutableState<String?> = mutableStateOf(null),
    val held: MutableState<String> = mutableStateOf(""),
) {
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)

    /** A clear, block or delete in flight; the menus wait for it. */
    var busy by mutableStateOf(false)
    var cleared by mutableStateOf(false)

    /** Loads [message] into the composer; a text typed before waits in [held]. */
    fun edit(message: ChatMessage) {
        if (editing.value == null) held.value = draft.value
        draft.value = message.text.orEmpty()
        editing.value = messageKey(message)
        status = null
    }

    /** Ends an edit, or a sent draft: [held] is empty unless an edit put a typed text there. */
    fun restoreDraft() {
        draft.value = held.value
        held.value = ""
        editing.value = null
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
    val form = rememberChatForm()
    val scope = rememberCoroutineScope()
    LaunchedEffect(feed.version) {
        withContext(Dispatchers.IO) { runCatching { feed.history(peer) } }
            .onSuccess { shown ->
                if (history == null) form.status = null
                history = shown
                // Opening reads the chat, and so does a text received while it stays open; the polls
                // stop in the background, so receipts go out only while the chat is on screen.
                withContext(Dispatchers.IO) { feed.markRead(peer) }
            }
            .onFailure {
                if (history == null) form.status = ChatStatus(R.string.chat_load_failed, it.toString())
            }
    }
    BackHandler(onBack = onBack)
    val actions = ChatActions(
        onSend = { scope.sendDraft(form, feed, peer, history?.messages.orEmpty()) },
        onProfile = onProfile,
        onBack = onBack,
        onClear = { scope.clearChat(form, feed, peer) },
        onBlock = { scope.blockPeer(form, feed, peer, onBack) },
        onMute = { history?.let { scope.mutePeer(form, feed, peer, !it.muted) } },
        onDelete = {
            history?.messages.orEmpty().byKey(form.deleting.value)?.let { scope.deleteMessage(form, feed, peer, it) }
        },
    )
    ChatContent(peer, opened.peerId, history, form, actions)
}

/**
 * The chat as shown: the state's notices, the thread, the composer and the menu's dialogs. Stateless,
 * so that tests drive every chat state without a network.
 */
@Composable
@Suppress("LongParameterList") // Who talks, what is shown, what is typed and what the controls do.
internal fun ChatContent(peer: String, me: String, history: ChatHistory?, form: ChatForm, actions: ChatActions) {
    val state = history?.state
    // An edit goes out only while its message can still change, and only with a changed text.
    val edited = history?.messages.orEmpty().byKey(form.editing.value)
    val ready = form.editing.value == null ||
        edited != null && changeable(edited, me) && edited.text != form.draft.value
    Column(Modifier.fillMaxSize().safeDrawingPadding().testTag("chat")) {
        ChatBackBar(peer, actions) { ChatMoreMenu(form, history, actions.onMute) }
        ChatStateNotice(state, form)
        ChatThread(me, history, form, Modifier.weight(1f))
        Column(Modifier.padding(horizontal = 16.dp), Arrangement.spacedBy(8.dp)) {
            if (state == ChatState.REQUEST_SENT) Caption(stringResource(R.string.chat_first_wait))
            ChatStatusLine(form)
            if (chatTooLong(form.draft.value)) Caption(stringResource(R.string.chat_too_long))
            if (form.editing.value != null) EditingLine(form)
        }
        ChatComposer(state, form, ready, actions.onSend)
    }
    ChatConfirm(form, actions)
}

@Composable
private fun ChatBackBar(peer: String, actions: ChatActions, menu: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(8.dp), Arrangement.spacedBy(8.dp), Alignment.CenterVertically) {
        IconButton(onClick = actions.onBack, Modifier.testTag("chat-back")) {
            Icon(painterResource(R.drawable.ic_back), stringResource(R.string.nav_chats))
        }
        Row(Modifier.weight(1f).testTag("chat-profile").clickable(onClick = actions.onProfile),
            Arrangement.spacedBy(8.dp), Alignment.CenterVertically) {
            Avatar(peer)
            Text(shortPeerId(peer), Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium)
        }
        menu()
    }
}

// The one-message rule matters before that message too, so it also shows in a new conversation.
@Composable
private fun ChatStateNotice(state: ChatState?, form: ChatForm) {
    val modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp)
    when (state) {
        ChatState.NONE, ChatState.REQUEST_SENT ->
            Caption(stringResource(R.string.chat_first_notice), modifier.testTag("chat-first-notice"))
        ChatState.REQUEST_RECEIVED -> Surface(
            modifier.fillMaxWidth(), RoundedCornerShape(12.dp), MaterialTheme.colorScheme.tertiaryContainer
        ) {
            Column(Modifier.padding(12.dp).testTag("chat-request-notice"), Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.chat_request_notice))
                OutlinedButton({ form.dialog.value = ChatDialog.BLOCK }, Modifier.testTag("request-block"),
                    enabled = !form.busy) { Text(stringResource(R.string.chat_block)) }
            }
        }
        ChatState.CLOSED ->
            Caption(stringResource(R.string.chat_closed_notice), modifier.testTag("chat-closed-notice"))
        else -> Unit
    }
}

@Composable
private fun ChatThread(me: String, history: ChatHistory?, form: ChatForm, modifier: Modifier) {
    val messages = history?.messages.orEmpty()
    val cleared = form.cleared
    val writable = chatWritable(history?.state)
    val list = rememberLazyListState()
    // The cleared note comes first, so the last message is one item further.
    LaunchedEffect(messages.size) {
        if (messages.isNotEmpty()) list.scrollToItem(messages.lastIndex + if (cleared) 1 else 0)
    }
    LazyColumn(
        modifier.fillMaxWidth().testTag("chat-thread"),
        state = list,
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (cleared) item { Caption(stringResource(R.string.chat_cleared), Modifier.testTag("chat-cleared-note")) }
        items(messages) { message ->
            val own = message.author == me
            Box(Modifier.fillMaxWidth(), if (own) Alignment.CenterEnd else Alignment.CenterStart) {
                if (changeable(message, me)) {
                    MessageMenu(message, form, writable) { ChatBubble(message, true, it) }
                } else {
                    ChatBubble(message, own, Modifier)
                }
            }
        }
    }
}

@Composable
private fun ChatBubble(message: ChatMessage, own: Boolean, modifier: Modifier) {
    Surface(
        modifier.padding(start = if (own) 48.dp else 0.dp, end = if (own) 0.dp else 48.dp),
        RoundedCornerShape(12.dp),
        if (own) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
            Text(chatPreview(message))
            chatMeta(message).takeIf { it.isNotEmpty() }?.let { Caption(it) }
        }
    }
}

@Composable
private fun chatMeta(message: ChatMessage): String = listOfNotNull(
    chatTime(message.sentAtMs),
    stringResource(R.string.chat_edited).takeIf { message.edited && !message.deleted },
    message.delivery?.takeIf { !message.deleted }?.let {
        stringResource(if (message.read) R.string.chat_read else deliveryText(it))
    },
).joinToString(META_SEPARATOR)

// Node acceptance reads as Sent: only a device of the peer makes it Delivered, its receipt Read.
private fun deliveryText(delivery: ChatDelivery) = when (delivery) {
    ChatDelivery.PENDING -> R.string.chat_pending
    ChatDelivery.NODE_ACCEPTED -> R.string.chat_sent
    ChatDelivery.DEVICE_RECEIVED -> R.string.chat_delivered
}

@Composable
private fun ChatStatusLine(form: ChatForm) {
    val status = form.status
    val line = Modifier.testTag(STATUS_TAG).semantics { liveRegion = LiveRegionMode.Polite }
    when {
        form.sending -> Caption(stringResource(R.string.chat_sending), line)
        status == null -> Unit
        status.details == null -> Caption(stringResource(status.text), line)
        else -> ErrorNotice(stringResource(status.text), STATUS_TAG, status.details, "chat-details",
            "copy-chat-details")
    }
}

@Composable
private fun ChatComposer(state: ChatState?, form: ChatForm, ready: Boolean, onSend: () -> Unit) {
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
            enabled = !form.sending && ready && chatSendable(state, text),
        ) {
            Icon(painterResource(R.drawable.ic_send), stringResource(R.string.chat_send))
        }
    }
}
