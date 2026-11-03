package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatLikes
import uniffi.dyapp.FoundProfile

private const val NOT_CONNECTED = "notConnected"

/** The profile behind the page: being fetched, verified, returned by no node, or a failed fetch. */
internal sealed interface ProfileLoad {
    data object Loading : ProfileLoad
    data class Shown(val found: FoundProfile) : ProfileLoad
    data object Missing : ProfileLoad
    data class Failed(val status: ChatStatus) : ProfileLoad
}

/** The like controls: [likes] is null until read; the dialog and the last outcome are not saved. */
internal class ProfileForm {
    var likes by mutableStateOf<ChatLikes?>(null)
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)
    var confirming by mutableStateOf(false)
}

/** Where the page leads: [backLabel] names what back returns to. */
internal class ProfileActions(val backLabel: Int, val back: () -> Unit, val message: () -> Unit)

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

/** The fetch behind the page; a retry has something to do only until a profile is shown. */
@Composable
internal fun rememberProfileLoad(peer: String, known: FoundProfile?, feed: ChatFeed, attempt: Int): State<ProfileLoad> =
    produceState(known?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Loading, attempt) {
        if (value !is ProfileLoad.Shown) {
            value = ProfileLoad.Loading
            value = withContext(Dispatchers.IO) { runCatching { feed.profiles(listOf(peer)).firstOrNull() } }.fold(
                { found -> found?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Missing },
                { ProfileLoad.Failed(profileLoadStatus(it)) },
            )
        }
    }

/** Likes [peer] or takes the like back, and shows the outcome on [form]. */
internal fun CoroutineScope.sendLike(form: ProfileForm, feed: ChatFeed, peer: String, liked: Boolean) {
    form.sending = true
    form.status = null
    launch {
        val result = withContext(Dispatchers.IO) { feed.like(peer, liked) }
        form.sending = false
        // A failed send can still be staged, so the reread after feed.like decides the button.
        result.onSuccess { form.likes = it }.onFailure { form.status = likeStatus(it) }
    }
}
