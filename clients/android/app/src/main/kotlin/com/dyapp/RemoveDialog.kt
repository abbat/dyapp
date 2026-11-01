package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

/**
 * Asks before an identity's key and data leave this device: Remove stays disabled until the box is
 * ticked. A removal ends at the picker through [act], where a new identity is never made on its own.
 */
@Composable
internal fun RemoveDialog(slots: Slots, target: Opened, act: (() -> Opened) -> Unit, onDismiss: () -> Unit) {
    val (slot, peerId) = when (target) {
        is Opened.Ready -> target.slot to target.peerId
        is Opened.Recovery -> target.slot to target.peerId
        else -> return
    }
    val copies = remember(slot.id) {
        peerId != null && runCatching {
            slots.list().filter { it.id != slot.id }.map { openSlot(it, slots::load) }
                .any { it is Opened.Ready && it.peerId == peerId }
        }.getOrDefault(false)
    }
    var confirmed by remember(slot.id) { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.remove_title), Modifier.testTag("remove-title")) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                RemoveSummary(target, peerId, copies)
                Row(
                    Modifier.toggleable(confirmed, role = Role.Checkbox) { confirmed = it }.testTag("remove-confirm"),
                    Arrangement.spacedBy(8.dp),
                    Alignment.CenterVertically
                ) {
                    Checkbox(confirmed, onCheckedChange = null)
                    Text(stringResource(R.string.remove_confirm))
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { act { slots.remove(slot.id, peerId); Opened.Picker } },
                Modifier.testTag("remove-action"),
                enabled = confirmed,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)
            ) {
                Text(stringResource(R.string.remove_action))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, Modifier.testTag("remove-cancel")) {
                Text(stringResource(R.string.remove_cancel))
            }
        },
    )
}

@Composable
private fun RemoveSummary(target: Opened, peerId: String?, copies: Boolean) {
    if (peerId == null) {
        Text(reasonText((target as Opened.Recovery).reason))
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Avatar(peerId)
            Column {
                Text(shortPeerId(peerId), fontFamily = FontFamily.Monospace)
                Caption(stringResource(R.string.settings_key_store))
            }
        }
        Text(peerId, Modifier.testTag("remove-peer"), fontFamily = FontFamily.Monospace)
        Text(stringResource(if (copies) R.string.remove_other_copy else R.string.remove_last_copy))
    }
    // A missing key or another copy means nothing irreplaceable is lost.
    if (peerId != null && !copies) {
        Text(stringResource(R.string.remove_warning), Modifier.testTag("remove-warning"))
    }
    Text(stringResource(R.string.remove_loses))
}
