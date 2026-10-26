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
import androidx.compose.runtime.DisposableEffect
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
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow

private enum class DraftStatus { LOADING, EDITING, SAVING, SAVED, READ_FAILED, SAVE_FAILED }
private const val CHECK_WAIT_MS = 5_000

private class NetworkDraftState {
    var seeds by mutableStateOf("")
    var name by mutableStateOf("")
    var status by mutableStateOf(DraftStatus.LOADING)
    var checker by mutableStateOf<ConnectionCheck?>(null)
    var checkStatus by mutableStateOf<Int?>(null)
    var checkRows by mutableStateOf<List<Pair<String, SeedRow>>>(emptyList())

    fun editSeeds(value: String) { seeds = value; clearCheck() }
    fun editName(value: String) { name = value; clearCheck() }
    private fun clearCheck() { checkStatus = null; checkRows = emptyList() }
}

/** Whether this ready identity still needs its first network connection. */
@Composable
internal fun rememberNetworkEntryNeeded(opened: Opened.Ready?): Boolean? {
    var needed by remember(opened?.slot?.id) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(opened?.slot?.id) {
        if (opened == null) return@LaunchedEffect
        val loaded = withContext(Dispatchers.IO) {
            runCatching { RustBridge.loadNetwork(opened, NetworkFile.ACTIVE) }
        }
        needed = loaded.fold(onSuccess = { it == null }, onFailure = { true })
    }
    return needed
}

private class DraftActions(val check: () -> Unit, val save: () -> Unit)

private suspend fun loadDraft(opened: Opened.Ready, state: NetworkDraftState) {
    val loaded = withContext(Dispatchers.IO) {
        runCatching {
            val settings = RustBridge.loadNetwork(opened, NetworkFile.DRAFT)
                ?: RustBridge.loadNetwork(opened, NetworkFile.ACTIVE)
            settings to RustBridge.connectionCheck(opened)
        }
    }
    loaded.fold(onSuccess = { (settings, checker) ->
        state.seeds = settings?.seeds?.joinToString("\n").orEmpty()
        state.name = settings?.name.orEmpty()
        state.checker = checker
        state.status = DraftStatus.EDITING
    }, onFailure = { state.status = DraftStatus.READ_FAILED })
}

private suspend fun checkDraft(checker: ConnectionCheck, draft: NetworkSettings, state: NetworkDraftState) {
    val checked = withContext(Dispatchers.IO) {
        runCatching { checker.run(draft, CHECK_WAIT_MS.toUInt()) }
    }
    checked.fold(onSuccess = { rows ->
        state.checkRows = draft.seeds.zip(rows)
        state.checkStatus = checkSummary(rows)
    }, onFailure = { state.checkStatus = checkFailure(it) })
}

private suspend fun saveDraft(opened: Opened.Ready, draft: NetworkSettings, state: NetworkDraftState) {
    val saved = withContext(Dispatchers.IO) { runCatching { RustBridge.saveNetworkDraft(opened, draft) } }
    state.status = if (saved.isSuccess) DraftStatus.SAVED else DraftStatus.SAVE_FAILED
}

/** Edit one ready identity's network draft; checking and activation are separate steps. */
@Composable
internal fun NetworkDraftScreen(opened: Opened.Ready, onBack: () -> Unit) {
    val state = remember(opened.slot.id) { NetworkDraftState() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(opened.slot.id) { loadDraft(opened, state) }
    DisposableEffect(state.checker) {
        val checker = state.checker
        onDispose { checker?.cancel(); checker?.close() }
    }
    val actions = DraftActions(check = {
        state.checker?.let { checker ->
            state.checkStatus = R.string.network_checking
            state.checkRows = emptyList()
            scope.launch { checkDraft(checker, networkSettings(state), state) }
        }
    }, save = {
        state.status = DraftStatus.SAVING
        scope.launch { saveDraft(opened, networkSettings(state), state) }
    })
    NetworkDraftForm(opened.peerId, state, onBack, actions)
}

private fun networkSettings(state: NetworkDraftState) = NetworkSettings(
    state.seeds.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.toList(),
    state.name
)

@Composable
private fun NetworkDraftForm(peerId: String, state: NetworkDraftState, onBack: () -> Unit, actions: DraftActions) {
    val editable = state.status !in setOf(DraftStatus.LOADING, DraftStatus.SAVING, DraftStatus.READ_FAILED) &&
        state.checkStatus != R.string.network_checking
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.network_title))
        Text(stringResource(R.string.storage_peer_id))
        Text(peerId, Modifier.testTag("network-peer-id"))
        Text(stringResource(R.string.storage_os_loss))
        NetworkDraftFields(state, editable)
        Button(onClick = actions.save, enabled = editable, modifier = Modifier.testTag("network-save")) {
            Text(stringResource(R.string.network_save))
        }
        Button(onClick = actions.check, enabled = editable && state.checker != null &&
            state.checkStatus != R.string.network_checking, modifier = Modifier.testTag("network-check")) {
            Text(stringResource(R.string.network_check))
        }
        Text(stringResource(networkStatusText(state.status)), Modifier.testTag("network-status"))
        NetworkCheckResults(state.checkStatus, state.checkRows)
        OutlinedButton(onClick = onBack, modifier = Modifier.testTag("network-back")) {
            Text(stringResource(R.string.profile_back))
        }
    }
}

@Composable
private fun NetworkDraftFields(state: NetworkDraftState, editable: Boolean) {
    OutlinedTextField(state.seeds, { state.editSeeds(it); state.status = DraftStatus.EDITING }, enabled = editable,
        label = { Text(stringResource(R.string.network_seeds)) },
        modifier = Modifier.testTag("network-seeds"))
    OutlinedTextField(state.name, { state.editName(it); state.status = DraftStatus.EDITING }, enabled = editable,
        label = { Text(stringResource(R.string.network_name)) },
        modifier = Modifier.testTag("network-name"))
}

private fun networkStatusText(status: DraftStatus): Int = when (status) {
    DraftStatus.LOADING -> R.string.network_loading
    DraftStatus.EDITING -> R.string.network_draft_only
    DraftStatus.SAVING -> R.string.network_saving
    DraftStatus.SAVED -> R.string.network_saved
    DraftStatus.READ_FAILED -> R.string.network_read_failed
    DraftStatus.SAVE_FAILED -> R.string.network_save_failed
}
