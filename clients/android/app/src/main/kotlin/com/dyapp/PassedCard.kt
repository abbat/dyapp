package com.dyapp

import androidx.compose.foundation.layout.Arrangement
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

// What the card shows: null peers while loading; [busy] while an undo runs.
private class PassedState {
    var peers by mutableStateOf<List<String>?>(null)
    var failed by mutableStateOf(false)
    var busy by mutableStateOf(false)
}

// Undoes the pass on [peer] when given, then reads the list; a failure keeps the last list.
private suspend fun PassedState.refresh(active: Opened.Ready, peer: String? = null) {
    busy = true
    val result = withContext(Dispatchers.IO) {
        runCatching {
            RustBridge.withChats(active) { chats ->
                peer?.let { chats.setPassed(it, false) }
                chats.passedPeers()
            }
        }
    }
    busy = false
    failed = result.isFailure
    result.onSuccess { peers = it }
}

/** The Settings card listing the people the active identity passed on, each with Undo pass. */
@Composable
internal fun PassedCard(active: Opened.Ready) {
    val scope = rememberCoroutineScope()
    val state = remember(active.slot.id) { PassedState() }
    LaunchedEffect(active.slot.id) { state.refresh(active) }
    FormCard(Modifier.testTag("passed")) {
        CardTitle(R.string.settings_passed)
        val peers = state.peers
        peers?.forEachIndexed { index, peer ->
            PassedRow(peer, index + 1, enabled = !state.busy) { scope.launch { state.refresh(active, peer) } }
        }
        if (peers?.isEmpty() == true) {
            Caption(stringResource(R.string.settings_passed_none), Modifier.testTag("passed-none"))
        }
        if (state.failed) {
            Caption(stringResource(R.string.settings_passed_failed),
                Modifier.testTag("passed-error").semantics { liveRegion = LiveRegionMode.Polite })
        }
        Caption(stringResource(R.string.settings_passed_hint))
    }
}

@Composable
private fun PassedRow(peer: String, number: Int, enabled: Boolean, onUndo: () -> Unit) {
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(12.dp), Alignment.CenterVertically) {
        Avatar(peer)
        Text(shortPeerId(peer), Modifier.weight(1f), fontFamily = FontFamily.Monospace)
        TextButton(onClick = onUndo, Modifier.testTag("unpass-$number"), enabled = enabled) {
            Text(stringResource(R.string.settings_unpass))
        }
    }
}
