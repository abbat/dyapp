package com.dyapp

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

/** What a bottom bar item or button opens. */
internal enum class StartTarget { REMOVE, PROFILE, SEARCH, NETWORK, SETTINGS, LOCK }

/** First-run step 1: explains the key and creates nothing until [onCreate]. */
@Composable
internal fun WizardIdentity(onCreate: () -> Unit) {
    Page(
        actions = {
            Button(onClick = onCreate, Modifier.fillMaxWidth().testTag("create-identity")) {
                Text(stringResource(R.string.wizard_create))
            }
        },
    ) {
        Caption(stringResource(R.string.wizard_step1))
        PageTitle(R.string.wizard_identity_title)
        Text(stringResource(R.string.wizard_identity_intro))
        Caption(stringResource(R.string.storage_android), Modifier.testTag("key-store"))
        Caption(stringResource(R.string.backup_none), Modifier.testTag("no-backup"))
    }
}

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

/** The start screen without a ready identity: the picker, recovery or a storage error. */
@Composable
internal fun StartColumn(shown: Shown, slots: Slots, act: (() -> Opened) -> Unit, open: (StartTarget) -> Unit) {
    val opened = shown.opened
    Page(actions = { StartActions(opened, slots, act, open) }) {
        PageTitle(startStatus(opened), Modifier.testTag(APP_READY_TAG))
        when (opened) {
            null -> Text(stringResource(R.string.storage_error))
            is Opened.Recovery -> RecoveryDetails(opened)
            else -> PickerList(shown.entries, slots, act)
        }
    }
}

@Composable
private fun RecoveryDetails(opened: Opened.Recovery) {
    ErrorNotice(
        reasonText(opened.reason), "recovery-reason",
        listOfNotNull(opened.reason, opened.peerId).joinToString("\n"), "recovery-details", "copy-recovery-details"
    )
    opened.peerId?.let {
        Caption(stringResource(R.string.storage_peer_id))
        Text(it, Modifier.testTag("recovery-peer"), fontFamily = FontFamily.Monospace)
    }
}

@Composable
private fun PickerList(entries: List<Opened>, slots: Slots, act: (() -> Opened) -> Unit) {
    Text(stringResource(R.string.picker_intro), color = MaterialTheme.colorScheme.onSurfaceVariant)
    Column(Modifier.testTag("identities")) {
        for ((index, entry) in entries.withIndex()) {
            val (slot, peerId, label) = when (entry) {
                is Opened.Ready -> Triple(entry.slot, entry.peerId, stringResource(R.string.settings_key_store))
                is Opened.Recovery -> Triple(entry.slot, entry.peerId, reasonText(entry.reason))
                else -> continue
            }
            val select = { slots.setActive(slot.id); slots.open() }
            Row(
                Modifier.fillMaxWidth().heightIn(min = 72.dp).clickable { act(select) }.testTag("pick-${index + 1}"),
                Arrangement.spacedBy(16.dp),
                Alignment.CenterVertically
            ) {
                Avatar(peerId.orEmpty())
                Column {
                    peerId?.let { Text(shortPeerId(it), fontFamily = FontFamily.Monospace) }
                    Caption(label)
                }
            }
        }
    }
}

// Lock settings stay reachable while no identity opens: the Settings tab needs a ready one.
@Composable
private fun StartActions(opened: Opened?, slots: Slots, act: (() -> Opened) -> Unit, open: (StartTarget) -> Unit) {
    val wide = Modifier.fillMaxWidth()
    when (opened) {
        null -> {}
        is Opened.Recovery -> {
            Button(onClick = { act(slots::open) }, wide.testTag("retry")) {
                Text(stringResource(R.string.recovery_retry))
            }
            OutlinedButton(onClick = { act { Opened.Picker } }, wide.testTag("switch-identity")) {
                Text(stringResource(R.string.identity_switch))
            }
            if (removable(opened)) {
                TextButton(
                    onClick = { open(StartTarget.REMOVE) },
                    wide.testTag("remove-identity"),
                    colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)
                ) { Text(stringResource(R.string.recovery_remove)) }
            }
        }
        else -> OutlinedButton(onClick = { act(slots::create) }, wide.testTag("add-identity")) {
            Text(stringResource(R.string.identity_add))
        }
    }
    TextButton(onClick = { open(StartTarget.LOCK) }, wide.testTag("lock-settings")) {
        Text(stringResource(R.string.lock_settings))
    }
}

private fun startStatus(opened: Opened?) = when (opened) {
    null -> R.string.status_error
    is Opened.Recovery -> R.string.status_recovery
    else -> R.string.status_pick
}
