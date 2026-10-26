package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings

private enum class DraftStatus { LOADING, EDITING, SAVING, SAVED, READ_FAILED, SAVE_FAILED }

private class NetworkDraftState {
    var seeds by mutableStateOf("")
    var name by mutableStateOf("")
    var status by mutableStateOf(DraftStatus.LOADING)
}

/** Edit one ready identity's network draft; checking and activation are separate steps. */
@Composable
internal fun NetworkDraftScreen(opened: Opened.Ready, onBack: () -> Unit) {
    val state = remember(opened.slot.id) { NetworkDraftState() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(opened.slot.id) {
        val loaded = withContext(Dispatchers.IO) {
            runCatching {
                RustBridge.loadNetwork(opened, NetworkFile.DRAFT)
                    ?: RustBridge.loadNetwork(opened, NetworkFile.ACTIVE)
            }
        }
        loaded.fold(
            onSuccess = {
                state.seeds = it?.seeds?.joinToString("\n").orEmpty()
                state.name = it?.name.orEmpty()
                state.status = DraftStatus.EDITING
            },
            onFailure = { state.status = DraftStatus.READ_FAILED }
        )
    }
    NetworkDraftForm(opened.peerId, state, onBack) {
        state.status = DraftStatus.SAVING
        scope.launch {
            val draft = networkSettings(state)
            val saved = withContext(Dispatchers.IO) { runCatching { RustBridge.saveNetworkDraft(opened, draft) } }
            state.status = if (saved.isSuccess) DraftStatus.SAVED else DraftStatus.SAVE_FAILED
        }
    }
}

private fun networkSettings(state: NetworkDraftState) = NetworkSettings(
    state.seeds.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.toList(),
    state.name
)

@Composable
private fun NetworkDraftForm(peerId: String, state: NetworkDraftState, onBack: () -> Unit, onSave: () -> Unit) {
    val editable = state.status !in setOf(DraftStatus.LOADING, DraftStatus.SAVING, DraftStatus.READ_FAILED)
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.network_title))
        Text(stringResource(R.string.storage_peer_id))
        Text(peerId, Modifier.testTag("network-peer-id"))
        Text(stringResource(R.string.storage_os_loss))
        OutlinedTextField(state.seeds, { state.seeds = it; state.status = DraftStatus.EDITING }, enabled = editable,
            label = { Text(stringResource(R.string.network_seeds)) },
            modifier = Modifier.testTag("network-seeds"))
        OutlinedTextField(state.name, { state.name = it; state.status = DraftStatus.EDITING }, enabled = editable,
            label = { Text(stringResource(R.string.network_name)) },
            modifier = Modifier.testTag("network-name"))
        Button(onClick = onSave, enabled = editable, modifier = Modifier.testTag("network-save")) {
            Text(stringResource(R.string.network_save))
        }
        Text(stringResource(networkStatusText(state.status)), Modifier.testTag("network-status"))
        OutlinedButton(onClick = onBack, modifier = Modifier.testTag("network-back")) {
            Text(stringResource(R.string.profile_back))
        }
    }
}

private fun networkStatusText(status: DraftStatus): Int = when (status) {
    DraftStatus.LOADING -> R.string.network_loading
    DraftStatus.EDITING -> R.string.network_draft_only
    DraftStatus.SAVING -> R.string.network_saving
    DraftStatus.SAVED -> R.string.network_saved
    DraftStatus.READ_FAILED -> R.string.network_read_failed
    DraftStatus.SAVE_FAILED -> R.string.network_save_failed
}
