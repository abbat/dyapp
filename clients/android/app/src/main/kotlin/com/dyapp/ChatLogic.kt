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

internal fun sendStatus(error: Throwable, staged: Boolean): ChatStatus {
    val details = error.toString()
    return when {
        staged -> ChatStatus(R.string.chat_queued, null)
        "notConnected" in details -> ChatStatus(R.string.search_not_connected, details)
        "unpublished" in details -> ChatStatus(R.string.chat_unpublished, details)
        else -> ChatStatus(R.string.chat_send_failed, details)
    }
}

/** Sends the draft; only a failure that stored nothing keeps the text for another try. */
internal fun CoroutineScope.sendDraft(form: ChatForm, feed: ChatFeed, peer: String, before: List<ChatMessage>) {
    val text = form.draft.value
    form.sending = true
    form.status = null
    launch {
        val status = withContext(Dispatchers.IO) { feed.send(peer, text, before) }
        form.sending = false
        form.status = status
        if (status?.details == null) form.draft.value = ""
    }
}
