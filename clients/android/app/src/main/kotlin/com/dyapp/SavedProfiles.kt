package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

private class SavedPresentation {
    var peers by mutableStateOf<List<String>?>(null)
    var busy by mutableStateOf(true)
    var message by mutableStateOf<Int?>(null)
    var active = true

    fun admits(peer: String?): Boolean {
        if (!active || busy) return false
        return peer == null || peer in peers.orEmpty()
    }

    fun complete(result: Result<List<String>>, removing: String?) {
        result.fold(
            onSuccess = {
                peers = it
                message = null
            },
            onFailure = {
                message = if (removing == null) R.string.favorites_list_failed else R.string.favorites_change_failed
            }
        )
        busy = false
    }
}

/** Search's private collection: opening the list reads no network profiles or media. */
@Composable
internal fun SavedProfiles(
    owner: Any,
    expanded: MutableState<Boolean>,
    calls: SavedProfilesCalls,
    open: (String) -> Unit,
) {
    FormCard {
        TextButton(
            { expanded.value = !expanded.value },
            Modifier.fillMaxWidth().testTag("saved-profiles-toggle")
        ) {
            Text(stringResource(if (expanded.value) R.string.favorites_hide else R.string.favorites_show))
        }
        if (expanded.value) key(owner) { SavedProfilesList(calls, open) }
    }
}

@Composable
private fun SavedProfilesList(calls: SavedProfilesCalls, open: (String) -> Unit) {
    val state = remember { SavedPresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    DisposableEffect(state) { onDispose { state.active = false } }
    LaunchedEffect(Unit) { state.complete(backend.refresh(), null) }
    val refresh = { peer: String? ->
        if (state.admits(peer)) {
            state.busy = true
            state.message = null
            scope.launch { state.complete(backend.refresh(peer), peer) }
        }
    }
    SavedStatus(state)
    SavedProfileRows(state, open) { refresh(it) }
    OutlinedButton(
        { refresh(null) },
        Modifier.fillMaxWidth().testTag("saved-profiles-refresh"),
        enabled = !state.busy
    ) {
        Text(stringResource(R.string.favorites_refresh))
    }
}

@Composable
private fun SavedStatus(state: SavedPresentation) {
    Caption(stringResource(R.string.favorites_list_hint))
    if (state.busy) Caption(stringResource(R.string.favorites_list_loading))
    state.message?.let {
        Text(
            stringResource(it),
            Modifier.testTag("saved-profiles-status").semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
    if (state.peers?.isEmpty() == true) Caption(stringResource(R.string.favorites_empty))
}

@Composable
private fun SavedProfileRows(state: SavedPresentation, open: (String) -> Unit, remove: (String) -> Unit) {
    val peers = state.peers.orEmpty()
    // This helper reads only local/cached labels; deliberately do not call FetchPeerProfiles.
    val names = rememberContactNames(peers)
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        for (peer in peers) key(peer) {
            val label = contactLabel(peer, names[peer])
            val removeLabel = stringResource(R.string.favorites_remove_named, label)
            Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp)) {
                TextButton(
                    { if (state.admits(peer)) open(peer) },
                    Modifier.weight(1f).testTag("saved-open-$peer"),
                    enabled = !state.busy
                ) { Text(label) }
                TextButton(
                    { remove(peer) },
                    Modifier.testTag("saved-remove-$peer").semantics { contentDescription = removeLabel },
                    enabled = !state.busy
                ) { Text(stringResource(R.string.favorites_remove)) }
            }
        }
    }
}
