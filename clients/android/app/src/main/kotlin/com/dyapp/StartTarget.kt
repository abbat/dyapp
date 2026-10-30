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

/** What a start screen button opens. */
internal enum class StartTarget { REMOVE, PROFILE, SEARCH, NETWORK, SETTINGS, LOCK }

/**
 * Shows the editor opened from the start screen, or the network draft while no network is
 * active; returns false when neither applies, so the caller shows the start screen.
 */
@Composable
internal fun showReadyEditor(slots: Slots, opened: Opened.Ready?, editor: StartTarget?, close: () -> Unit): Boolean {
    val networkNeeded = rememberNetworkEntryNeeded(opened)
    // Survives rotation, so Back keeps the draft closed for this session.
    var networkDismissed by rememberSaveable(opened?.slot?.id) { mutableStateOf(false) }
    val automatic = networkNeeded != false && !networkDismissed
    if (opened == null || editor == null && !automatic) return false
    when {
        editor == StartTarget.PROFILE -> ProfileDraftScreen(opened, close)
        editor == StartTarget.SEARCH -> SearchScreen(opened, close)
        networkNeeded == null -> Text(stringResource(R.string.network_loading))
        else -> NetworkDraftScreen(opened) {
            close()
            networkDismissed = true
        }
    }
    return true
}

/** The start screen column: status, identity details and the buttons that open other screens. */
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
        IdentityDetails(shown, slots, act) { open(StartTarget.PROFILE) }
        StartButtons(opened, act, open)
    }
}

// Remove, switch, search, network, settings and lock settings, as far as the opened state allows.
@Composable
private fun StartButtons(opened: Opened?, act: (() -> Opened) -> Unit, open: (StartTarget) -> Unit) {
    if (opened != null && removable(opened)) {
        OutlinedButton(onClick = { open(StartTarget.REMOVE) }, Modifier.testTag("remove-identity")) {
            Text(stringResource(R.string.identity_remove))
        }
    }
    if (opened is Opened.Ready || opened is Opened.Recovery) {
        OutlinedButton(onClick = { act { Opened.Picker } }, Modifier.testTag("switch")) {
            Text(stringResource(R.string.identity_switch))
        }
    }
    if (opened is Opened.Ready) {
        OutlinedButton(onClick = { open(StartTarget.SEARCH) }, Modifier.testTag("search-open")) {
            Text(stringResource(R.string.search_title))
        }
        OutlinedButton(onClick = { open(StartTarget.NETWORK) }, Modifier.testTag("network-draft-open")) {
            Text(stringResource(R.string.network_title))
        }
        OutlinedButton(onClick = { open(StartTarget.SETTINGS) }, Modifier.testTag("settings-open")) {
            Text(stringResource(R.string.settings_title))
        }
    }
    OutlinedButton(onClick = { open(StartTarget.LOCK) }, Modifier.testTag("lock-settings")) {
        Text(stringResource(R.string.lock_settings))
    }
}

private fun startStatus(opened: Opened?) = when (opened) {
    null -> R.string.status_error
    is Opened.Ready -> R.string.status_ready
    is Opened.Recovery -> R.string.status_recovery
    else -> R.string.status_pick
}
