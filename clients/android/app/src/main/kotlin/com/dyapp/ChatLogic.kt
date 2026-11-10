package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState
import uniffi.dyapp.chatComposerPolicy

/** Shared composer policy; null means the conversation is still loading. */
internal fun chatWritable(state: ChatState?) = chatComposerPolicy(state, "").writable

internal fun chatTooLong(text: String) = chatComposerPolicy(null, text).tooLong

internal fun chatSendable(state: ChatState?, text: String) = chatComposerPolicy(state, text).sendable

/** Localized presentation of an authoritative native outcome; no history-based inference. */
internal fun sendStatus(
    result: Result<uniffi.dyapp.ChatSubmission>,
    queued: Int = R.string.chat_queued,
    failed: Int = R.string.chat_send_failed,
): ChatStatus? = result.fold(
    onSuccess = { if (it is uniffi.dyapp.ChatSubmission.Queued) ChatStatus(queued, null) else null },
    onFailure = {
        val details = it.toString()
        when {
            "notConnected" in details -> ChatStatus(R.string.search_not_connected, details)
            "unpublished" in details -> ChatStatus(R.string.chat_unpublished, details)
            else -> ChatStatus(failed, details)
        }
    },
)

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
        val result = withContext(Dispatchers.IO) {
            if (editing == null) feed.send(peer, text) else feed.edit(peer, editing, text)
        }
        form.sending = false
        form.completeSubmission(result, editing != null)
    }
}

/** Restore the held draft only after the native operation committed. */
internal fun ChatForm.completeSubmission(result: Result<uniffi.dyapp.ChatSubmission>, editing: Boolean) {
    status = sendStatus(
        result,
        if (editing) R.string.chat_edit_queued else R.string.chat_queued,
        if (editing) R.string.chat_edit_failed else R.string.chat_send_failed,
    )
    if (result.isSuccess) restoreDraft()
}

/** Deletes the confirmed message; a failure says why, and the thread rereads either way. */
internal fun CoroutineScope.deleteMessage(form: ChatForm, feed: ChatFeed, peer: String, message: ChatMessage) {
    form.busy = true
    launch {
        val result = withContext(Dispatchers.IO) { feed.delete(peer, message) }
        form.status = sendStatus(result, R.string.chat_delete_queued, R.string.chat_delete_failed)
        form.busy = false
        // A committed deletion has nothing left to edit.
        if (result.isSuccess && form.editing.value == messageKey(message)) form.restoreDraft()
    }
}
