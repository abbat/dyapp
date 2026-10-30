package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/**
 * The identity list of the active identity's Settings screen. Create and Switch change the active
 * slot through [act]; Remove, offered only where [removable], opens the confirmation screen and
 * Back (a null target) closes Settings through [open].
 */
@Composable
internal fun IdentitySettingsScreen(
    slots: Slots,
    active: Opened.Ready,
    act: (() -> Opened) -> Unit,
    open: (StartTarget?) -> Unit
) {
    val listed = remember(slots, active.slot.id) { RustBridge.show(slots) { Opened.Picker } }
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.settings_title), style = MaterialTheme.typography.headlineLarge)
        Text(stringResource(R.string.settings_identities))
        if (listed.opened == null) {
            Text(stringResource(R.string.storage_error))
        } else {
            listed.entries.forEach { entry ->
                IdentitySettingsRow(entry, active.slot.id) { id -> act { slots.setActive(id); slots.open() } }
            }
        }
        Button(onClick = { act(slots::create) }, Modifier.testTag("settings-create")) {
            Text(stringResource(R.string.identity_add))
        }
        if (removable(active)) {
            OutlinedButton(onClick = { open(StartTarget.REMOVE) }, Modifier.testTag("settings-remove")) {
                Text(stringResource(R.string.identity_remove))
            }
        }
        OutlinedButton(onClick = { open(null) }, modifier = Modifier.testTag("settings-back")) {
            Text(stringResource(R.string.profile_back))
        }
    }
}

@Composable
private fun IdentitySettingsRow(entry: Opened, activeSlotId: String, switch: (String) -> Unit) {
    val (slot, label) = when (entry) {
        is Opened.Ready -> entry.slot to entry.peerId
        is Opened.Recovery -> entry.slot to (entry.peerId ?: entry.slot.id)
        else -> return
    }
    Column(Modifier.testTag("settings-identity")) {
        Text(label)
        Text(stringResource(R.string.settings_key_store))
        if (entry is Opened.Recovery) Text(stringResource(R.string.status_recovery))
        if (slot.id == activeSlotId) {
            Text(stringResource(R.string.settings_active), Modifier.testTag("settings-active"))
        } else {
            OutlinedButton(onClick = { switch(slot.id) }, Modifier.testTag("settings-switch")) {
                Text(stringResource(R.string.settings_switch))
            }
        }
    }
}
