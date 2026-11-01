package com.dyapp

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatLikes
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.profileScales

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
        "notConnected" in details -> ChatStatus(R.string.view_like_not_connected, details)
        else -> ChatStatus(R.string.view_like_failed, details)
    }
}

internal fun profileLoadStatus(error: Throwable): ChatStatus {
    val details = error.toString()
    val text = if ("notConnected" in details) R.string.search_not_connected else R.string.view_load_failed
    return ChatStatus(text, details)
}

// A retry has something to do only until a profile is shown.
@Composable
private fun rememberProfileLoad(peer: String, known: FoundProfile?, feed: ChatFeed, attempt: Int): State<ProfileLoad> =
    produceState(known?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Loading, attempt) {
        if (value !is ProfileLoad.Shown) {
            value = ProfileLoad.Loading
            value = withContext(Dispatchers.IO) { runCatching { feed.profile(peer) } }.fold(
                { found -> found?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Missing },
                { ProfileLoad.Failed(profileLoadStatus(it)) },
            )
        }
    }

private fun CoroutineScope.sendLike(form: ProfileForm, feed: ChatFeed, peer: String, liked: Boolean) {
    form.sending = true
    form.status = null
    launch {
        val result = withContext(Dispatchers.IO) { feed.like(peer, liked) }
        form.sending = false
        // A failed send can still be staged, so the reread after feed.like decides the button.
        result.onSuccess { form.likes = it }.onFailure { form.status = likeStatus(it) }
    }
}

/**
 * A found profile without the app bars. [known] is the result already verified; without one the page
 * fetches the profile. The likes reread with [feed], so a like from the peer shows on an open page.
 */
@Composable
@Suppress("LongParameterList") // The peer, its known profile, the chats and the routes are all distinct.
internal fun ProfileViewScreen(
    opened: Opened.Ready,
    peer: String,
    known: FoundProfile?,
    feed: ChatFeed,
    actions: ProfileActions,
) {
    var attempt by remember { mutableIntStateOf(0) }
    val load by rememberProfileLoad(peer, known, feed, attempt)
    val form = remember { ProfileForm() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(feed.version) {
        withContext(Dispatchers.IO) { runCatching { feed.likes(peer) } }
            .onSuccess { form.likes = it }
            .onFailure { if (form.likes == null) form.status = profileLoadStatus(it) }
    }
    BackHandler(onBack = actions.back)
    ProfileViewContent(peer, peer == opened.peerId, load, form, actions, { scope.sendLike(form, feed, peer, it) }) {
        attempt += 1
    }
}

/** The page as shown; the own profile has nothing to like or to write to. */
@Composable
@Suppress("LongParameterList") // Stateless so that tests drive every state without a network.
internal fun ProfileViewContent(
    peer: String,
    own: Boolean,
    load: ProfileLoad,
    form: ProfileForm,
    actions: ProfileActions,
    onLike: (Boolean) -> Unit,
    onRetry: () -> Unit,
) {
    Column(Modifier.fillMaxSize().safeDrawingPadding().testTag("profile-view")) {
        IconButton(onClick = actions.back, Modifier.padding(8.dp).testTag("view-back")) {
            Icon(painterResource(R.drawable.ic_back), stringResource(actions.backLabel))
        }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp),
            Arrangement.spacedBy(12.dp),
        ) {
            Text(shortPeerId(peer), Modifier.semantics { heading() }, style = MaterialTheme.typography.headlineSmall)
            Caption(stringResource(R.string.search_peer, peer), Modifier.testTag("view-peer"))
            CopyButton(peer, "copy-view-peer")
            if (!own) ProfileLikes(form, actions.message, onLike)
            ProfileBody(load, onRetry)
        }
    }
    if (form.confirming) {
        UnlikeDialog({ form.confirming = false }) {
            form.confirming = false
            onLike(false)
        }
    }
}

@Composable
private fun ProfileLikes(form: ProfileForm, onMessage: () -> Unit, onLike: (Boolean) -> Unit) {
    val ours = form.likes?.ours == true
    val matched = form.likes?.matched == true
    val ready = form.likes != null && !form.sending
    if (matched) {
        FormCard(Modifier.testTag("view-match").semantics { liveRegion = LiveRegionMode.Polite }) {
            Text(stringResource(R.string.view_match), style = MaterialTheme.typography.titleSmall)
            Caption(stringResource(R.string.view_match_text))
        }
    }
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp)) {
        val side = Modifier.weight(1f)
        if (ours) {
            FilledTonalButton({ form.confirming = true }, side.testTag("view-unlike"), enabled = ready) {
                HeartLabel(R.drawable.ic_heart_filled, R.string.view_liked)
            }
        }
        if (matched) {
            Button(onMessage, side.testTag("view-message-match")) { Text(stringResource(R.string.view_message)) }
        } else {
            OutlinedButton(onMessage, side.testTag("view-message")) { Text(stringResource(R.string.view_message)) }
        }
        if (!ours) {
            Button({ onLike(true) }, side.testTag("view-like"), enabled = ready) {
                HeartLabel(R.drawable.ic_heart, R.string.view_like)
            }
        }
    }
    if (!matched) Caption(stringResource(R.string.view_first_hint))
    if (ours) Caption(stringResource(R.string.view_unlike_hint))
    LikeStatusLine(form)
}

@Composable
private fun HeartLabel(icon: Int, text: Int) {
    Icon(painterResource(icon), null, Modifier.padding(end = 8.dp))
    Text(stringResource(text))
}

@Composable
private fun LikeStatusLine(form: ProfileForm) {
    val status = form.status
    when {
        form.sending -> Caption(stringResource(R.string.view_sending),
            Modifier.testTag("view-status").semantics { liveRegion = LiveRegionMode.Polite })
        status != null -> ErrorNotice(stringResource(status.text), "view-status", status.details.orEmpty(),
            "view-details", "copy-view-details")
    }
}

@Composable
private fun ProfileBody(load: ProfileLoad, onRetry: () -> Unit) {
    when (load) {
        ProfileLoad.Loading -> Caption(stringResource(R.string.view_loading),
            Modifier.testTag("view-load").semantics { liveRegion = LiveRegionMode.Polite })
        ProfileLoad.Missing -> {
            Caption(stringResource(R.string.view_not_found), Modifier.testTag("view-load"))
            ProfileRetry(onRetry)
        }
        is ProfileLoad.Failed -> {
            ErrorNotice(stringResource(load.status.text), "view-load", load.status.details.orEmpty(),
                "view-load-details", "copy-view-load-details")
            ProfileRetry(onRetry)
        }
        is ProfileLoad.Shown -> {
            val scales = remember { profileScales() }
            FormCard(Modifier.testTag("view-fields")) {
                ProfileRows(load.found.draft.draft(), scales, LocalContext.current)
            }
        }
    }
}

@Composable
private fun ProfileRetry(onRetry: () -> Unit) {
    OutlinedButton(onRetry, Modifier.fillMaxWidth().testTag("view-retry")) {
        Text(stringResource(R.string.search_retry))
    }
}

@Composable
private fun UnlikeDialog(onCancel: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.unlike_title), Modifier.testTag("unlike-title")) },
        text = { Text(stringResource(R.string.unlike_text)) },
        confirmButton = {
            TextButton(onConfirm, Modifier.testTag("unlike-confirm")) { Text(stringResource(R.string.unlike_action)) }
        },
        dismissButton = {
            TextButton(onCancel, Modifier.testTag("unlike-cancel")) { Text(stringResource(R.string.remove_cancel)) }
        },
    )
}
