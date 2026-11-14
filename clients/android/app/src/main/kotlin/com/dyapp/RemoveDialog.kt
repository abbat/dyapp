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
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ProfilePublisher
import uniffi.dyapp.profileVisibility

private const val REMOVE_TITLE_TAG = "remove-title"
private const val REMOVE_CANCEL_TAG = "remove-cancel"

/** How far a removal got; a running withdrawal cannot be dismissed. */
private class RemoveState {
    var confirmed by mutableStateOf(false)
    var busy by mutableStateOf(false)
    var offline by mutableStateOf(false)

    /** The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway. */
    suspend fun withdrawThen(opened: Opened.Ready, remove: () -> Unit) {
        busy = true
        val result = withContext(Dispatchers.IO) {
            runCatching {
                withSearchOutboxChange {
                    RustBridge.withKey(
                        opened
                    ) { dir, key -> ProfilePublisher(dir).use { it.withdraw(key) } }
                }
            }
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
    if (slot == null) return
    val checked by rememberNativeRead(slots, target) {
        val copies = hasOtherCopy(slots, slot.id, peerId)
        val published = (target as? Opened.Ready)?.takeIf {
            RustBridge.withKey(it) { dir, key -> profileVisibility(dir, key).version > 0uL }
        }
        RemovalView(target, slot to peerId, copies, published)
    }
    val view = checked?.getOrNull()
    if (view == null) {
        RemovalWaiting(checked != null, onDismiss)
    } else {
        key(target) { ReadyRemovalDialog(slots, view, act, onDismiss) }
    }
}

private data class RemovalView(
    val target: Opened,
    val identity: Pair<Slot, String?>,
    val copies: Boolean,
    val published: Opened.Ready?
)

@Composable
private fun ReadyRemovalDialog(
    slots: Slots,
    view: RemovalView,
    act: (() -> Opened) -> Unit,
    onDismiss: () -> Unit
) {
    val (slot, peerId) = view.identity
    val published = view.published
    val state = remember(slot.id) { RemoveState() }
    val scope = rememberCoroutineScope()
    val remove: () -> Unit = {
        act {
            slots.remove(slot.id, peerId)
            Opened.Picker
        }
    }
    val proceed: () -> Unit = {
        if (published == null) remove() else scope.launch { state.withdrawThen(published, remove) }
    }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(R.string.remove_title), Modifier.testTag(REMOVE_TITLE_TAG)) },
        text = { state.RemoveText(view.target, peerId, view.copies, published != null) },
        confirmButton = { RemoveActions(state, proceed, remove) },
        dismissButton = { RemoveCancel(state, onDismiss) },
    )
}

@Composable
private fun RemovalWaiting(failed: Boolean, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.remove_title), Modifier.testTag(REMOVE_TITLE_TAG)) },
        text = {
            Text(
                stringResource(if (failed) R.string.storage_error else R.string.view_loading),
                Modifier.testTag(if (failed) "remove-read-failed" else "remove-loading")
            )
        },
        confirmButton = {},
        dismissButton = {
            TextButton(onDismiss, Modifier.testTag(REMOVE_CANCEL_TAG)) { Text(stringResource(R.string.remove_cancel)) }
        }
    )
}

@Composable
private fun RemoveState.RemoveText(target: Opened, peerId: String?, copies: Boolean, published: Boolean) {
    Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        RemoveSummary(target, peerId, copies, published)
        RemoveChoice(this@RemoveText)
    }
}

private fun hasOtherCopy(slots: Slots, slotId: String, peerId: String?) = peerId != null &&
    slots.list().filter { it.id != slotId }.map { slots.inspect(it.id) }
        .any { it is Opened.Ready && it.peerId == peerId }

@Composable
private fun RemoveCancel(state: RemoveState, onDismiss: () -> Unit) {
    TextButton(
        onClick = onDismiss,
        Modifier.testTag(if (state.offline) "remove-cancel-offline" else REMOVE_CANCEL_TAG),
        enabled = !state.busy,
    ) {
        Text(stringResource(R.string.remove_cancel))
    }
}

@Composable
private fun RemoveChoice(state: RemoveState) {
    if (state.offline) {
        Text(
            stringResource(R.string.remove_offline),
            Modifier.testTag("remove-offline")
                .semantics { liveRegion = LiveRegionMode.Polite }
        )
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
