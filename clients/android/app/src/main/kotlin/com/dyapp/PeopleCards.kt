package com.dyapp

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatRevocation
import uniffi.dyapp.ChatSession

// What a people card shows: null peers while loading; [busy] while a change runs; [status] after a retry.
private class PeopleState {
    var peers by mutableStateOf<List<String>?>(null)
    var failed by mutableStateOf(false)
    var busy by mutableStateOf(false)
    var status by mutableStateOf<String?>(null)
}

// Runs [change] when given, then reads the list with [list]; a failure keeps the last list.
private suspend fun PeopleState.refresh(
    active: Opened.Ready,
    list: (ChatSession) -> List<String>,
    change: ((ChatSession) -> Unit)? = null
) {
    busy = true
    val result = withContext(Dispatchers.IO) {
        runCatching { RustBridge.withChats(active) { chats -> change?.invoke(chats); list(chats) } }
    }
    busy = false
    failed = result.isFailure
    result.onSuccess { peers = it }
}

// Blocks [peer] again to repeat the token revocation and says what it came to, with the reason when it failed.
private suspend fun PeopleState.retry(active: Opened.Ready, peer: String, text: (Int) -> String) {
    busy = true
    status = null
    val result = withContext(Dispatchers.IO) { runCatching { RustBridge.withChats(active) { it.block(peer) } } }
    busy = false
    status = result.fold(
        { listOfNotNull(text(revocationText(it.revocation)), it.detail).joinToString(" ") },
        { "${text(R.string.settings_blocked_retry_failed)} $it" }
    )
}

/** The Settings card listing the people the active identity blocked, each with Unblock and a revocation retry. */
@Composable
internal fun BlockedCard(active: Opened.Ready) {
    val scope = rememberCoroutineScope()
    val state = remember(active.slot.id) { PeopleState() }
    val list = { chats: ChatSession -> chats.blockedPeers() }
    val texts = PeopleTexts(R.string.settings_blocked, R.string.settings_blocked_none, R.string.settings_blocked_failed,
        R.string.settings_blocked_hint)
    val context = LocalContext.current
    val text = { id: Int -> context.getString(id) }
    LaunchedEffect(state) { state.refresh(active, list) }
    PeopleCard("blocked", state, texts) { peer, number ->
        // Stacked: two long labels beside the Peer ID do not fit a 360 dp phone.
        Column(horizontalAlignment = Alignment.End) {
            TextButton({ scope.launch { state.retry(active, peer, text) } },
                Modifier.testTag("retry-revocation-$number"), enabled = !state.busy) {
                Text(stringResource(R.string.settings_blocked_retry))
            }
            TextButton({ scope.launch { state.refresh(active, list) { it.unblock(peer) } } },
                Modifier.testTag("unblock-$number"), enabled = !state.busy) {
                Text(stringResource(R.string.settings_unblock))
            }
        }
    }
}

/** The Settings card listing the people the active identity passed on, each with Undo pass. */
@Composable
internal fun PassedCard(active: Opened.Ready) {
    val scope = rememberCoroutineScope()
    val state = remember(active.slot.id) { PeopleState() }
    val list = { chats: ChatSession -> chats.passedPeers() }
    val texts = PeopleTexts(R.string.settings_passed, R.string.settings_passed_none, R.string.settings_passed_failed,
        R.string.settings_passed_hint)
    LaunchedEffect(state) { state.refresh(active, list) }
    PeopleCard("passed", state, texts) { peer, number ->
        TextButton({ scope.launch { state.refresh(active, list) { it.setPassed(peer, false) } } },
            Modifier.testTag("unpass-$number"), enabled = !state.busy) {
            Text(stringResource(R.string.settings_unpass))
        }
    }
}

// The title, empty, failure and hint texts of a people card.
private class PeopleTexts(
    @StringRes val title: Int,
    @StringRes val none: Int,
    @StringRes val failed: Int,
    @StringRes val hint: Int
)

// A card tagged [tag] that gives each peer row the [actions] for that peer and its number.
@Composable
private fun PeopleCard(
    tag: String,
    state: PeopleState,
    texts: PeopleTexts,
    actions: @Composable (String, Int) -> Unit
) {
    FormCard(Modifier.testTag(tag)) {
        CardTitle(texts.title)
        val peers = state.peers
        peers?.forEachIndexed { index, peer ->
            Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(12.dp), Alignment.CenterVertically) {
                Avatar(peer)
                Text(shortPeerId(peer), Modifier.weight(1f), fontFamily = FontFamily.Monospace)
                actions(peer, index + 1)
            }
        }
        if (peers?.isEmpty() == true) {
            Caption(stringResource(texts.none), Modifier.testTag("$tag-none"))
        }
        if (state.failed) {
            Caption(stringResource(texts.failed),
                Modifier.testTag("$tag-error").semantics { liveRegion = LiveRegionMode.Polite })
        }
        state.status?.let {
            Caption(it, Modifier.testTag("$tag-revoke-status").semantics { liveRegion = LiveRegionMode.Polite })
        }
        Caption(stringResource(texts.hint))
    }
}

/** What a block's token revocation came to, as Chats and the blocked people list say it. */
internal fun revocationText(revocation: ChatRevocation) = when (revocation) {
    ChatRevocation.PENDING -> R.string.chats_blocked_pending
    ChatRevocation.NOT_NEEDED -> R.string.chats_blocked_not_needed
    ChatRevocation.CONFIRMED -> R.string.chats_blocked_confirmed
}
