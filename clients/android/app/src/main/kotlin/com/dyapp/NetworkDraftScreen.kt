package com.dyapp

import android.util.Log
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
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
    var checkError by mutableStateOf<String?>(null)

    fun editSeeds(value: String) { seeds = value; clearCheck() }
    fun editName(value: String) { name = value; clearCheck() }
    private fun clearCheck() { checkStatus = null; checkRows = emptyList(); checkError = null }
}

private class DraftActions(
    val check: () -> Unit, val cancel: () -> Unit,
    val activate: () -> Unit, val save: () -> Unit
)

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
    }, onFailure = {
        Log.w("dyapp", "Connection check failed", it)
        state.checkStatus = checkFailure(it)
        state.checkError = it.toString()
    })
}

private suspend fun activateDraft(checker: ConnectionCheck, state: NetworkDraftState) {
    val activated = withContext(Dispatchers.IO) { runCatching { checker.activate() } }
    state.checkStatus = if (activated.isSuccess) R.string.network_active else R.string.network_activation_failed
    state.checkError = activated.exceptionOrNull()?.toString()
}

private suspend fun saveDraft(opened: Opened.Ready, draft: NetworkSettings, state: NetworkDraftState) {
    val saved = withContext(Dispatchers.IO) { runCatching { RustBridge.saveNetworkDraft(opened, draft) } }
    state.status = if (saved.isSuccess) DraftStatus.SAVED else DraftStatus.SAVE_FAILED
}

/**
 * Edit one ready identity's network draft. The wizard ([onBack] set) checks and activates in separate
 * steps; the section does both with one action, and a failed check leaves the active network alone.
 */
@Composable
internal fun NetworkDraftScreen(opened: Opened.Ready, onBack: (() -> Unit)?) {
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
            state.checkError = null
            scope.launch {
                checkDraft(checker, networkSettings(state), state)
                if (onBack == null && state.checkStatus == R.string.network_connected) {
                    state.checkStatus = R.string.network_activating
                    activateDraft(checker, state)
                }
            }
        }
    }, cancel = { state.checker?.cancel() }, activate = {
        if (state.checkStatus == R.string.network_connected) {
            state.checker?.let { checker ->
                state.checkStatus = R.string.network_activating
                scope.launch { activateDraft(checker, state) }
            }
        }
    }, save = {
        state.status = DraftStatus.SAVING
        scope.launch { saveDraft(opened, networkSettings(state), state) }
    })
    NetworkDraftForm(opened, state, onBack, actions)
}

private fun networkSettings(state: NetworkDraftState) = NetworkSettings(
    state.seeds.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.toList(),
    state.name
)

@Composable
private fun NetworkDraftForm(opened: Opened.Ready, state: NetworkDraftState, onBack: (() -> Unit)?, actions: DraftActions) {
    val editable = state.status !in setOf(DraftStatus.LOADING, DraftStatus.SAVING, DraftStatus.READ_FAILED) &&
        state.checkStatus !in setOf(R.string.network_checking, R.string.network_activating)
    if (onBack != null) {
        Page(actions = { WizardNetworkActions(state, editable, onBack, actions) }) {
            Caption(stringResource(R.string.wizard_step2))
            PageTitle(R.string.wizard_network_title)
            Caption(stringResource(R.string.storage_peer_id))
            Text(opened.peerId, Modifier.testTag("peer-id"), fontFamily = FontFamily.Monospace)
            CopyButton(opened.peerId, "copy-peer-id")
            NetworkDraftFields(state, editable, wizard = true)
            Caption(stringResource(networkStatusText(state.status)), Modifier.testTag("network-status"))
            NetworkCheckResults(state.checkStatus, state.checkRows)
        }
        return
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        val active = activeNetworkStatus(opened, state.checkStatus == R.string.network_active)
        Caption(
            if (state.status == DraftStatus.READ_FAILED) stringResource(R.string.network_read_failed) else active,
            Modifier.testTag("network-status")
        )
        SectionCheckStatus(state)
        FormCard {
            NetworkDraftFields(state, editable, wizard = false)
            val checking = state.checkStatus == R.string.network_checking
            SeedRows(if (checking) networkSettings(state).seeds.map { it to null } else state.checkRows)
            if (checking || state.checkStatus == R.string.network_activating) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            if (checking) {
                OutlinedButton(onClick = actions.cancel, Modifier.fillMaxWidth().testTag("cancel-check")) {
                    Text(stringResource(R.string.network_cancel))
                }
            }
            Button(onClick = actions.check, Modifier.fillMaxWidth().testTag("check-and-connect"),
                editable && state.checker != null) {
                Text(stringResource(R.string.network_check_and_connect))
            }
        }
        Caption(stringResource(R.string.network_keep))
    }
}

// Progress and success stay a caption; only a result that kept the old network is a notice.
@Composable
private fun SectionCheckStatus(state: NetworkDraftState) {
    val status = state.checkStatus ?: return
    val calm = setOf(
        R.string.network_checking, R.string.network_activating, R.string.network_connected,
        R.string.network_active, R.string.network_cancelled,
    )
    if (status in calm) {
        Caption(
            stringResource(status),
            Modifier.testTag("network-check-status").semantics { liveRegion = LiveRegionMode.Polite }
        )
    } else {
        ErrorNotice(
            stringResource(status), "network-check-status",
            state.checkError ?: rowDetails(state.checkRows), "network-error-details", "copy-network-details"
        )
    }
}

// One primary action at a time: check, activate what the check connected, then leave.
@Composable
private fun WizardNetworkActions(state: NetworkDraftState, editable: Boolean, onBack: () -> Unit, actions: DraftActions) {
    val checked = state.checkStatus
    val wide = Modifier.fillMaxWidth()
    when (checked) {
        R.string.network_connected -> Button(onClick = actions.activate, wide.testTag("network-continue")) {
            Text(stringResource(R.string.network_continue))
        }
        R.string.network_active -> Button(onClick = onBack, wide.testTag("network-done")) {
            Text(stringResource(R.string.network_continue))
        }
        else -> {
            Button(onClick = actions.check, wide.testTag("check-connection"), editable && state.checker != null) {
                Text(stringResource(if (checked == null) R.string.network_check else R.string.network_retry))
            }
            if (checked == R.string.network_checking) {
                OutlinedButton(onClick = actions.cancel, wide.testTag("cancel-check")) {
                    Text(stringResource(R.string.network_cancel))
                }
            }
        }
    }
    if (checked != R.string.network_active) {
        TextButton(onClick = actions.save, wide.testTag("network-save"), editable) {
            Text(stringResource(R.string.network_save))
        }
        TextButton(onClick = onBack, wide.testTag("skip-network"), checked != R.string.network_activating) {
            Text(stringResource(R.string.wizard_skip))
        }
    }
}

@Composable
private fun NetworkDraftFields(state: NetworkDraftState, editable: Boolean, wizard: Boolean) {
    OutlinedTextField(state.name, { state.editName(it); state.status = DraftStatus.EDITING }, enabled = editable,
        label = { Text(stringResource(R.string.network_name)) },
        placeholder = { Text(stringResource(R.string.network_name_public)) },
        supportingText = { Text(stringResource(R.string.network_name_hint)) },
        modifier = Modifier.fillMaxWidth().testTag(if (wizard) "network-name" else "network-name-field"))
    OutlinedTextField(state.seeds, { state.editSeeds(it); state.status = DraftStatus.EDITING }, enabled = editable,
        label = { Text(stringResource(R.string.network_seeds)) },
        supportingText = { Text(stringResource(R.string.network_seeds_hint)) },
        modifier = Modifier.fillMaxWidth().testTag(if (wizard) "seeds" else "network-seeds"))
}

private fun networkStatusText(status: DraftStatus): Int = when (status) {
    DraftStatus.LOADING -> R.string.network_loading
    DraftStatus.EDITING -> R.string.network_draft_only
    DraftStatus.SAVING -> R.string.network_saving
    DraftStatus.SAVED -> R.string.network_saved
    DraftStatus.READ_FAILED -> R.string.network_read_failed
    DraftStatus.SAVE_FAILED -> R.string.network_save_failed
}
