package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/** What a bottom bar item or button opens. */
internal enum class StartTarget { REMOVE, PROFILE, SEARCH, NETWORK, SETTINGS, LOCK }

/**
 * Shows the first-run network wizard while [opened] has no active network and no section was
 * picked yet; returns false otherwise, so the caller shows the app shell. Back skips it.
 */
@Composable
internal fun showNetworkWizard(opened: Opened.Ready, section: StartTarget?): Boolean {
    val networkNeeded = rememberNetworkEntryNeeded(opened)
    // Survives rotation, so Back keeps the wizard closed for this session.
    var networkDismissed by rememberSaveable(opened.slot.id) { mutableStateOf(false) }
    if (section != null || networkNeeded == false || networkDismissed) return false
    if (networkNeeded == null) {
        Text(stringResource(R.string.network_loading))
    } else {
        NetworkDraftScreen(opened) { networkDismissed = true }
    }
    return true
}

/** The start screen without a ready identity: status, the picker or recovery, and remove, switch and lock. */
@Composable
internal fun StartColumn(shown: Shown, slots: Slots, act: (() -> Opened) -> Unit, open: (StartTarget) -> Unit) {
    val opened = shown.opened
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
        Text(stringResource(startStatus(opened)), modifier = Modifier.testTag(APP_READY_TAG))
        IdentityDetails(shown, slots, act)
        StartButtons(opened, act, open)
    }
}

// Remove, switch and lock settings, as far as the opened state allows.
@Composable
private fun StartButtons(opened: Opened?, act: (() -> Opened) -> Unit, open: (StartTarget) -> Unit) {
    if (opened != null && removable(opened)) {
        OutlinedButton(onClick = { open(StartTarget.REMOVE) }, Modifier.testTag("remove-identity")) {
            Text(stringResource(R.string.identity_remove))
        }
    }
    if (opened is Opened.Recovery) {
        OutlinedButton(onClick = { act { Opened.Picker } }, Modifier.testTag("switch")) {
            Text(stringResource(R.string.identity_switch))
        }
    }
    OutlinedButton(onClick = { open(StartTarget.LOCK) }, Modifier.testTag("lock-settings")) {
        Text(stringResource(R.string.lock_settings))
    }
}

private fun startStatus(opened: Opened?) = when (opened) {
    null -> R.string.status_error
    is Opened.Recovery -> R.string.status_recovery
    else -> R.string.status_pick
}
