package com.dyapp

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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

private const val STATUS_TAG = "network-status"
private const val CHECK_STATUS_TAG = "network-check-status"
private const val CANCEL_TAG = "cancel-check"

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
        onDispose {
            checker?.cancel()
            checker?.close()
        }
    }
    val actions = draftActions(opened, state, scope, activateOnConnect = onBack == null)
    if (onBack != null) WizardNetworkPage(opened, state, onBack, actions) else NetworkSection(opened, state, actions)
}

@Composable
private fun WizardNetworkPage(
    opened: Opened.Ready,
    state: NetworkDraftState,
    onBack: () -> Unit,
    actions: DraftActions,
) {
    Page(actions = { WizardNetworkActions(state, onBack, actions) }) {
        Caption(stringResource(R.string.wizard_step2))
        PageTitle(R.string.wizard_network_title)
        Caption(stringResource(R.string.storage_peer_id))
        Text(opened.peerId, Modifier.testTag("peer-id"), fontFamily = FontFamily.Monospace)
        CopyButton(opened.peerId, "copy-peer-id")
        NetworkDraftFields(state, wizard = true)
        Caption(stringResource(networkStatusText(state.status)), Modifier.testTag(STATUS_TAG))
        NetworkCheckResults(state.checkStatus, state.checkRows)
    }
}

@Composable
private fun NetworkSection(opened: Opened.Ready, state: NetworkDraftState, actions: DraftActions) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        val active = activeNetworkStatus(opened, state.checkStatus == R.string.network_active)
        Caption(
            if (state.status == DraftStatus.READ_FAILED) stringResource(R.string.network_read_failed) else active,
            Modifier.testTag(STATUS_TAG)
        )
        SectionCheckStatus(state)
        NetworkSectionForm(state, actions)
        Caption(stringResource(R.string.network_keep))
    }
}

@Composable
private fun NetworkSectionForm(state: NetworkDraftState, actions: DraftActions) = FormCard {
    NetworkDraftFields(state, wizard = false)
    val checking = state.checkStatus == R.string.network_checking
    SeedRows(if (checking) networkSettings(state).seeds.map { it to null } else state.checkRows)
    if (checking || state.checkStatus == R.string.network_activating) {
        LinearProgressIndicator(Modifier.fillMaxWidth())
    }
    if (checking) {
        OutlinedButton(onClick = actions.cancel, Modifier.fillMaxWidth().testTag(CANCEL_TAG)) {
            Text(stringResource(R.string.network_cancel))
        }
    }
    Button(
        onClick = actions.check,
        Modifier.fillMaxWidth().testTag("check-and-connect"),
        state.editable && state.checker != null
    ) {
        Text(stringResource(R.string.network_check_and_connect))
    }
}

// Progress and success stay a caption; only a result that kept the old network is a notice.
@Composable
private fun SectionCheckStatus(state: NetworkDraftState) {
    val status = state.checkStatus ?: return
    val calm = setOf(
        R.string.network_checking,
        R.string.network_activating,
        R.string.network_connected,
        R.string.network_active,
        R.string.network_cancelled,
    )
    if (status in calm) {
        Caption(
            stringResource(status),
            Modifier.testTag(CHECK_STATUS_TAG).semantics { liveRegion = LiveRegionMode.Polite }
        )
    } else {
        ErrorNotice(
            stringResource(status),
            CHECK_STATUS_TAG,
            state.checkError ?: rowDetails(state.checkRows),
            "network-error-details",
            "copy-network-details"
        )
    }
}

// One primary action at a time: check, activate what the check connected, then leave.
@Composable
private fun WizardNetworkActions(state: NetworkDraftState, onBack: () -> Unit, actions: DraftActions) {
    val checked = state.checkStatus
    val wide = Modifier.fillMaxWidth()
    when (checked) {
        R.string.network_connected -> {
            Button(onClick = actions.activate, wide.testTag("network-continue")) {
                Text(stringResource(R.string.network_continue))
            }
        }
        R.string.network_active -> {
            Button(onClick = onBack, wide.testTag("network-done")) {
                Text(stringResource(R.string.network_continue))
            }
        }
        else -> {
            WizardCheckActions(state, actions)
        }
    }
    if (checked != R.string.network_active) {
        TextButton(onClick = actions.save, wide.testTag("network-save"), state.editable) {
            Text(stringResource(R.string.network_save))
        }
        TextButton(onClick = onBack, wide.testTag("skip-network"), checked != R.string.network_activating) {
            Text(stringResource(R.string.wizard_skip))
        }
    }
}

@Composable
private fun WizardCheckActions(state: NetworkDraftState, actions: DraftActions) {
    val checked = state.checkStatus
    val wide = Modifier.fillMaxWidth()
    Button(onClick = actions.check, wide.testTag("check-connection"), state.editable && state.checker != null) {
        Text(stringResource(if (checked == null) R.string.network_check else R.string.network_retry))
    }
    if (checked == R.string.network_checking) {
        OutlinedButton(onClick = actions.cancel, wide.testTag(CANCEL_TAG)) {
            Text(stringResource(R.string.network_cancel))
        }
    }
}

@Composable
private fun NetworkDraftFields(state: NetworkDraftState, wizard: Boolean) {
    OutlinedTextField(
        state.name,
        {
            state.editName(it)
            state.status = DraftStatus.EDITING
        },
        enabled = state.editable,
        label = { Text(stringResource(R.string.network_name)) },
        placeholder = { Text(stringResource(R.string.network_name_public)) },
        supportingText = { Text(stringResource(R.string.network_name_hint)) },
        modifier = Modifier.fillMaxWidth().testTag(if (wizard) "network-name" else "network-name-field")
    )
    OutlinedTextField(
        state.seeds,
        {
            state.editSeeds(it)
            state.status = DraftStatus.EDITING
        },
        enabled = state.editable,
        label = { Text(stringResource(R.string.network_seeds)) },
        supportingText = { Text(stringResource(R.string.network_seeds_hint)) },
        modifier = Modifier.fillMaxWidth().testTag(if (wizard) "seeds" else "network-seeds")
    )
}
