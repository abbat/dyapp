package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

// As the core's limit and the desktop composer.
private const val MAX_TEXT_BYTES = 65_536

/**
 * A shown chat takes text unless the first message waits for a reply or a match ended; null is
 * still loading.
 */
internal fun chatWritable(state: ChatState?) =
    state != null && state != ChatState.REQUEST_SENT && state != ChatState.CLOSED

internal fun chatTooLong(text: String) = text.toByteArray().size > MAX_TEXT_BYTES

internal fun chatSendable(state: ChatState?, text: String) =
    chatWritable(state) && text.isNotBlank() && !chatTooLong(text)

/** Whether a failed send still stored its text: another copy from the same draft must not go out. */
internal fun stagedNew(before: List<ChatMessage>, after: List<ChatMessage>, me: String): Boolean {
    val known = before.map { it.id.toList() }.toSet()
    return after.any { it.author == me && it.id.toList() !in known }
}

/** A failure's line; a change that was [staged] reads as [queued] instead, and keeps no details. */
internal fun sendStatus(
    error: Throwable,
    staged: Boolean,
    queued: Int = R.string.chat_queued,
    failed: Int = R.string.chat_send_failed,
): ChatStatus {
    val details = error.toString()
    return when {
        staged -> ChatStatus(queued, null)
        "notConnected" in details -> ChatStatus(R.string.search_not_connected, details)
        "unpublished" in details -> ChatStatus(R.string.chat_unpublished, details)
        else -> ChatStatus(failed, details)
    }
}

/** A message's saveable key: its group and ID, as a rotation must keep the edit and delete targets. */
internal fun messageKey(message: ChatMessage) = listOf(message.groupId, message.id)
    .joinToString(":") { bytes -> bytes.joinToString("") { "%02x".format(it) } }

internal fun List<ChatMessage>.byKey(key: String?) = key?.let { find { message -> messageKey(message) == it } }

/** Whether [message] has the menu: only an own text that was not deleted can change. */
internal fun changeable(message: ChatMessage, me: String) = message.author == me && !message.deleted

/**
 * Sends the draft, or the edit of the message [ChatForm.editing] names; only a failure that stored
 * nothing keeps the text for another try.
 */
internal fun CoroutineScope.sendDraft(form: ChatForm, feed: ChatFeed, peer: String, before: List<ChatMessage>) {
    val text = form.draft.value
    val editing = before.byKey(form.editing.value)
    // An edit whose message is gone must not go out as a new text.
    if (form.editing.value != null && editing == null) return
    form.sending = true
    form.status = null
    launch {
        val status = withContext(Dispatchers.IO) {
            if (editing == null) feed.send(peer, text, before) else feed.edit(peer, editing, text)
        }
        form.sending = false
        form.status = status
        if (status?.details == null) form.restoreDraft()
    }
}

/** Deletes the confirmed message; a failure says why, and the thread rereads either way. */
internal fun CoroutineScope.deleteMessage(form: ChatForm, feed: ChatFeed, peer: String, message: ChatMessage) {
    form.busy = true
    launch {
        val status = withContext(Dispatchers.IO) { feed.delete(peer, message) }
        form.status = status
        form.busy = false
        // A deleted message has nothing left to edit.
        if (status?.details == null && form.editing.value == messageKey(message)) form.restoreDraft()
    }
}
