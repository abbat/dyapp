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
import androidx.compose.runtime.remember
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

// The identities on this device: the active one, the others to switch to, and Add identity.
/** Switch an identity or explicitly create one from the menu. */
@Composable
internal fun IdentityMenu(slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) {
    val listed = remember(slots, active.slot.id) { RustBridge.show(slots) { Opened.Picker } }
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.settings_identities), style = MaterialTheme.typography.titleMedium)
        listed.entries.forEachIndexed { index, entry ->
            val (slot, label) = when (entry) {
                is Opened.Ready -> entry.slot to entry.peerId
                is Opened.Recovery -> entry.slot to (entry.peerId ?: reasonText(entry.reason))
                else -> return@forEachIndexed
            }
            if (slot == null) return@forEachIndexed
            if (slot.id == active.slot.id) {
                ActiveIdentity(label)
            } else {
                TextButton(
                    onClick = {
                        act { slots.choose(slot.id) }
                    },
                    Modifier.testTag("switch-to-${index + 1}")
                ) { Text(label) }
            }
        }
        TextButton(onClick = { act(slots::create) }, Modifier.testTag("add-identity-menu")) {
            Text(stringResource(R.string.identity_add))
        }
    }
}
