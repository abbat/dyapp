package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatLikes

private const val NOT_CONNECTED = "notConnected"

/**
 * The like and pass controls: [likes] is null until read; [status] is a failure and [info] a plain
 * outcome. The dialog and the last outcome are not saved.
 */
internal class ProfileForm {
    var likes by mutableStateOf<ChatLikes?>(null)
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)
    var info by mutableStateOf<Int?>(null)
    var confirming by mutableStateOf(false)
    var blocking by mutableStateOf(false)
    var readVersion by mutableIntStateOf(0)

    val likeReady: Boolean get() = likes != null && !sending
}

/** Where the page leads: [backLabel] names what back returns to; [blocked] leaves after a block. */
internal class ProfileActions(
    val backLabel: Int,
    val back: () -> Unit,
    val message: () -> Unit,
    val blocked: () -> Unit = back,
)

internal fun likeStatus(error: Throwable): ChatStatus {
    val details = error.toString()
    return when {
        "unpublished" in details -> ChatStatus(R.string.view_like_unpublished, details)
        NOT_CONNECTED in details -> ChatStatus(R.string.view_like_not_connected, details)
        else -> ChatStatus(R.string.view_like_failed, details)
    }
}

internal fun profileLoadStatus(error: Throwable): ChatStatus {
    val details = error.toString()
    val text = if (NOT_CONNECTED in details) R.string.search_not_connected else R.string.view_load_failed
    return ChatStatus(text, details)
}

/** Likes [peer] or takes the like back, and shows the outcome on [form]. */
internal fun CoroutineScope.sendLike(form: ProfileForm, feed: ChatFeed, peer: String, liked: Boolean) {
    if (!form.likeReady) return
    val request = form.start()
    launch {
        val result = withContext(Dispatchers.IO) { feed.like(peer, liked) }
        form.completeLike(request, result)
    }
}

/** Passes on [peer] or undoes that, and shows the outcome on [form]. */
internal fun CoroutineScope.sendPass(form: ProfileForm, feed: ChatFeed, peer: String, passed: Boolean) {
    if (!form.likeReady) return
    val request = form.start()
    launch {
        val result = withContext(Dispatchers.IO) { feed.pass(peer, passed) }
        if (!form.finish(request)) return@launch
        result.onSuccess {
            form.likes = it
            form.info = if (passed) R.string.view_passed else R.string.view_unpassed
        }.onFailure { form.status = ChatStatus(R.string.view_pass_failed, it.toString()) }
    }
}

/** Blocks [peer] and leaves through [onBlocked]; a failure stays on the page and says why. */
internal fun CoroutineScope.sendBlock(form: ProfileForm, feed: ChatFeed, peer: String, onBlocked: () -> Unit) {
    if (form.sending) return
    val request = form.start()
    launch {
        val result = withContext(Dispatchers.IO) { feed.block(peer) }
        if (!form.finish(request)) return@launch
        result.onSuccess { onBlocked() }
            .onFailure { form.status = ChatStatus(R.string.chat_block_failed, it.toString()) }
    }
}

internal fun ProfileForm.start(): Int {
    readVersion += 1
    sending = true
    status = null
    info = null
    return readVersion
}

/** Retire the operation before admitting a new read or action. */
internal fun ProfileForm.finish(request: Int): Boolean {
    if (request != readVersion) return false
    readVersion += 1
    sending = false
    return true
}

/** Only the current local attempt can finish its controls. */
internal fun ProfileForm.completeLike(request: Int, result: Result<uniffi.dyapp.ChatLikeAttempt>) {
    if (!finish(request)) return
    result.onSuccess {
        info = if (it.submission is uniffi.dyapp.ChatSubmission.Queued) R.string.view_like_pending else null
        when (val state = it.state) {
            is uniffi.dyapp.ChatLikeState.Ready -> { likes = state.likes }
            is uniffi.dyapp.ChatLikeState.Unavailable -> {
                likes = null
                status = ChatStatus(R.string.view_like_state_unavailable, state.detail)
            }
        }
    }.onFailure { status = likeStatus(it) }
}

/** Read completions cannot replace a newer operation or its projection. */
internal fun ProfileForm.finishRead(request: Int, result: Result<ChatLikes>) {
    if (sending || request != readVersion) return
    result.onSuccess {
        likes = it
        if (status?.text == R.string.view_like_state_unavailable) status = null
    }.onFailure { if (likes == null && status == null) status = profileLoadStatus(it) }
}
