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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ProfilePublisher

/** How far a removal got; a running withdrawal cannot be dismissed. */
private class RemoveState {
    var confirmed by mutableStateOf(false)
    var busy by mutableStateOf(false)
    var offline by mutableStateOf(false)

    /** The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway. */
    suspend fun withdrawThen(opened: Opened.Ready, remove: () -> Unit) {
        busy = true
        val result = withContext(Dispatchers.IO) {
            runCatching { RustBridge.withKey(opened) { dir, key -> ProfilePublisher(dir).use { it.withdraw(key) } } }
        }
        busy = false
        if (result.isSuccess) remove() else offline = true
    }
}

/**
 * Asks before an identity's key and data leave this device: Remove stays disabled until the box is
 * ticked. A removal ends at the picker through [act], where a new identity is never made on its own;
 * Settings keeps the active identity open when the removed one is another.
 */
@Composable
internal fun RemoveDialog(slots: Slots, target: Opened, act: (() -> Opened) -> Unit, onDismiss: () -> Unit) {
    val (slot, peerId) = when (target) {
        is Opened.Ready -> target.slot to target.peerId
        is Opened.Recovery -> target.slot to target.peerId
        else -> return
    }
    val copies = remember(slot.id) { hasOtherCopy(slots, slot.id, peerId) }
    // Only an open key can sign the withdrawal; an unreadable version counts as never published.
    val published = remember(slot.id) { (target as? Opened.Ready)?.takeIf { loadVisibility(it).version > 0uL } }
    val state = remember(slot.id) { RemoveState() }
    val scope = rememberCoroutineScope()
    val remove: () -> Unit = { act { slots.remove(slot.id, peerId); Opened.Picker } }
    val proceed: () -> Unit = {
        if (published == null) remove() else scope.launch { state.withdrawThen(published, remove) }
    }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(R.string.remove_title), Modifier.testTag("remove-title")) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                RemoveSummary(target, peerId, copies, published != null)
                RemoveChoice(state)
            }
        },
        confirmButton = { RemoveActions(state, proceed, remove) },
        dismissButton = { RemoveCancel(state, onDismiss) },
    )
}

private fun hasOtherCopy(slots: Slots, slotId: String, peerId: String?) = peerId != null && runCatching {
    slots.list().filter { it.id != slotId }.map { openSlot(it, slots::load) }
        .any { it is Opened.Ready && it.peerId == peerId }
}.getOrDefault(false)

@Composable
private fun RemoveCancel(state: RemoveState, onDismiss: () -> Unit) {
    TextButton(
        onClick = onDismiss,
        Modifier.testTag(if (state.offline) "remove-cancel-offline" else "remove-cancel"),
        enabled = !state.busy,
    ) {
        Text(stringResource(R.string.remove_cancel))
    }
}

@Composable
private fun RemoveChoice(state: RemoveState) {
    if (state.offline) {
        Text(stringResource(R.string.remove_offline), Modifier.testTag("remove-offline")
            .semantics { liveRegion = LiveRegionMode.Polite })
    } else {
        Row(
            Modifier.toggleable(state.confirmed, role = Role.Checkbox) { state.confirmed = it }
                .testTag("remove-confirm"),
            Arrangement.spacedBy(8.dp),
            Alignment.CenterVertically
        ) {
            Checkbox(state.confirmed, onCheckedChange = null)
            Text(stringResource(R.string.remove_confirm))
        }
    }
}

@Composable
private fun RemoveActions(state: RemoveState, proceed: () -> Unit, remove: () -> Unit) {
    val danger = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)
    if (state.offline) {
        // Stacked: three text buttons do not fit one row of the narrowest dialog.
        Column(horizontalAlignment = Alignment.End) {
            TextButton(proceed, Modifier.testTag("remove-retry"), enabled = !state.busy) {
                Text(stringResource(R.string.remove_retry))
            }
            TextButton(remove, Modifier.testTag("remove-anyway"), enabled = !state.busy, colors = danger) {
                Text(stringResource(R.string.remove_anyway))
            }
        }
    } else {
        val ready = state.confirmed && !state.busy
        TextButton(proceed, Modifier.testTag("remove-action"), enabled = ready, colors = danger) {
            Text(stringResource(R.string.remove_action))
        }
    }
}

@Composable
private fun RemoveSummary(target: Opened, peerId: String?, copies: Boolean, withdraws: Boolean) {
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
    if (withdraws) Text(stringResource(R.string.remove_withdraws))
    Text(stringResource(R.string.remove_loses))
}
