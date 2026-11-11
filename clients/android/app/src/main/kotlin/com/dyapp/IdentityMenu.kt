package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

@Composable
private fun ActiveIdentity(label: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(label, Modifier.weight(1f))
        Text(stringResource(R.string.settings_active), style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
private fun IdentityMenuEntry(entry: Opened, activeId: String, number: Int, select: (String) -> Unit) {
    val (slot, label) = when (entry) {
        is Opened.Ready -> entry.slot to entry.peerId
        is Opened.Recovery -> entry.slot to (entry.peerId ?: reasonText(entry.reason))
        else -> return
    }
    if (slot == null) return
    if (slot.id == activeId) {
        ActiveIdentity(label)
    } else {
        TextButton(onClick = { select(slot.id) }, Modifier.testTag("switch-to-$number")) { Text(label) }
    }
}

// The identities on this device: the active one, the others to switch to, and Add identity.
/** Switch an identity or explicitly create one from the menu. */
@Composable
internal fun IdentityMenu(slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) {
    val loaded by rememberNativeRead(slots, active) { RustBridge.show(slots) { Opened.Picker } }
    val listed = loaded?.getOrNull()
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.settings_identities), style = MaterialTheme.typography.titleMedium)
        if (loaded == null) Caption(stringResource(R.string.view_loading))
        if (loaded != null && listed?.opened == null) {
            Text(stringResource(R.string.storage_error))
        }
        listed?.entries.orEmpty().forEachIndexed { index, entry ->
            IdentityMenuEntry(entry, active.slot.id, index + 1) { id -> act { slots.choose(id) } }
        }
        TextButton(onClick = { act(slots::create) }, Modifier.testTag("add-identity-menu")) {
            Text(stringResource(R.string.identity_add))
        }
    }
}
