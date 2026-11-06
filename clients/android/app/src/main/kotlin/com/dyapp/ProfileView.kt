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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.profileScales

private const val STATUS_TAG = "view-status"
private const val LOAD_TAG = "view-load"

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
    onShared: (String) -> Unit = {},
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
    val onLike = { liked: Boolean -> scope.sendLike(form, feed, peer, liked) }
    val onPass = { passed: Boolean -> scope.sendPass(form, feed, peer, passed) }
    val onBlock = { scope.sendBlock(form, feed, peer, actions.blocked) }
    ProfileViewContent(peer, peer == opened.peerId, load, form, actions, onLike, onPass, onBlock,
        share = { ProfileShareButton(peer, feed, onShared) }) { attempt += 1 }
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
    onPass: (Boolean) -> Unit,
    onBlock: () -> Unit,
    share: @Composable () -> Unit = {},
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
            if (load is ProfileLoad.Shown) share()
            if (!own) QuietActions(form, onPass)
        }
    }
    ProfileDialogs(form, onLike, onBlock)
}

// Unlike and Block ask first; the block dialog is the one the chat menu uses.
@Composable
private fun ProfileDialogs(form: ProfileForm, onLike: (Boolean) -> Unit, onBlock: () -> Unit) {
    if (form.confirming) {
        val cancel = { form.confirming = false }
        Confirm("unlike", R.string.unlike_title, listOf(R.string.unlike_text), R.string.unlike_action, cancel) {
            cancel()
            onLike(false)
        }
    }
    if (form.blocking) {
        val cancel = { form.blocking = false }
        Confirm("block", R.string.block_title, listOf(R.string.block_text), R.string.chat_block, cancel) {
            cancel()
            onBlock()
        }
    }
}

@Composable
private fun ProfileLikes(form: ProfileForm, onMessage: () -> Unit, onLike: (Boolean) -> Unit) {
    val ours = form.likes?.ours == true
    val matched = form.likes?.matched == true
    if (matched) {
        FormCard(Modifier.testTag("view-match").semantics { liveRegion = LiveRegionMode.Polite }) {
            Text(stringResource(R.string.view_match), style = MaterialTheme.typography.titleSmall)
            Caption(stringResource(R.string.view_match_text))
        }
    }
    LikeButtons(form, onMessage, onLike)
    if (!matched) Caption(stringResource(R.string.view_first_hint))
    if (ours) Caption(stringResource(R.string.view_unlike_hint))
    LikeStatusLine(form)
}

@Composable
private fun LikeButtons(form: ProfileForm, onMessage: () -> Unit, onLike: (Boolean) -> Unit) {
    val ours = form.likes?.ours == true
    val ready = form.likes != null && !form.sending
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp)) {
        val side = Modifier.weight(1f)
        if (ours) {
            FilledTonalButton({ form.confirming = true }, side.testTag("view-unlike"), enabled = ready) {
                HeartLabel(R.drawable.ic_heart_filled, R.string.view_liked)
            }
        }
        if (form.likes?.matched == true) {
            Button(onMessage, side.testTag("view-message-match")) { Text(stringResource(R.string.view_message)) }
        } else {
            OutlinedButton(onMessage, side.testTag("view-message")) { Text(stringResource(R.string.view_message)) }
        }
        if (!ours && form.likes?.passed != true) {
            Button({ onLike(true) }, side.testTag("view-like"), enabled = ready) {
                HeartLabel(R.drawable.ic_heart, R.string.view_like)
            }
        }
    }
}

// Pass and Block sit below the profile, apart from Like; Pass is not offered while a like stands.
@Composable
private fun QuietActions(form: ProfileForm, onPass: (Boolean) -> Unit) {
    val likes = form.likes
    val ready = !form.sending
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp)) {
        when {
            likes == null -> Unit
            likes.passed -> TextButton({ onPass(false) }, Modifier.testTag("view-unpass"), enabled = ready) {
                Text(stringResource(R.string.view_unpass))
            }
            !likes.ours -> TextButton({ onPass(true) }, Modifier.testTag("view-pass"), enabled = ready) {
                Text(stringResource(R.string.view_pass))
            }
        }
        TextButton({ form.blocking = true }, Modifier.testTag("view-block"), enabled = ready) {
            Text(stringResource(R.string.chat_block))
        }
    }
}

@Composable
private fun HeartLabel(icon: Int, text: Int) {
    Icon(painterResource(icon), null, Modifier.padding(end = 8.dp))
    Text(stringResource(text))
}

@Composable
private fun LikeStatusLine(form: ProfileForm) {
    val status = form.status
    val info = form.info
    when {
        form.sending -> {
            Caption(stringResource(R.string.view_sending),
                Modifier.testTag(STATUS_TAG).semantics { liveRegion = LiveRegionMode.Polite })
        }
        status != null -> {
            ErrorNotice(stringResource(status.text), STATUS_TAG, status.details.orEmpty(),
                "view-details", "copy-view-details")
        }
        info != null -> {
            Caption(stringResource(info), Modifier.testTag(STATUS_TAG).semantics { liveRegion = LiveRegionMode.Polite })
        }
    }
}

@Composable
private fun ProfileBody(load: ProfileLoad, onRetry: () -> Unit) {
    when (load) {
        ProfileLoad.Loading -> {
            Caption(stringResource(R.string.view_loading),
                Modifier.testTag(LOAD_TAG).semantics { liveRegion = LiveRegionMode.Polite })
        }
        ProfileLoad.Missing -> {
            Caption(stringResource(R.string.view_not_found), Modifier.testTag(LOAD_TAG))
            ProfileRetry(onRetry)
        }
        is ProfileLoad.Failed -> {
            ErrorNotice(stringResource(load.status.text), LOAD_TAG, load.status.details.orEmpty(),
                "view-load-details", "copy-view-load-details")
            ProfileRetry(onRetry)
        }
        is ProfileLoad.Shown -> {
            val scales = remember { profileScales() }
            PeerGallery(load.found)
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
