package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.dyapp.LocalCollectionEntry

/** Local inventory paging never selects candidates or changes removal admission. */
@Composable
internal fun IdentityCollection(slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) = key(slots, active) {
    val state = remember { IdentityCollectionState() }
    val scope = rememberCoroutineScope()
    val read = { RustBridge.show(slots) { Opened.Picker } }
    val refresh: () -> Unit = { scope.launch { state.refresh(read) } }
    DisposableEffect(state) { onDispose { state.retire() } }
    LaunchedEffect(state) { state.refresh(read) }
    FormCard(Modifier.testTag("settings-identities")) {
        CardTitle(R.string.settings_identities)
        IdentityCollectionRows(state, active, refresh)
        if (state.failed) Text(stringResource(R.string.storage_error))
        Caption(stringResource(R.string.backup_none))
        Button(
            onClick = { if (state.present && !state.busy) act(slots::create) },
            Modifier.testTag("settings-create"),
            enabled = !state.busy
        ) { Text(stringResource(R.string.identity_add)) }
    }
    IdentityCollectionRemoval(slots, active, state, act)
}

@Composable
private fun IdentityCollectionRemoval(
    slots: Slots,
    active: Opened.Ready,
    state: IdentityCollectionState,
    act: (() -> Opened) -> Unit
) {
    val scope = rememberCoroutineScope()
    state.removing?.let { target ->
        val stay = (target as? Opened.Ready)?.slot?.id != active.slot.id
        RemoveDialog(slots, target, { action ->
            if (state.present) {
                state.dismiss()
                act { action().let { ended -> if (stay) slots.open() else ended } }
            }
        }) {
            state.dismiss()
            scope.launch { state.refresh { RustBridge.show(slots) { Opened.Picker } } }
        }
    }
}

private fun Opened.collectionTarget(): String? = when (this) {
    is Opened.Ready -> slot.id
    is Opened.Recovery -> id
    else -> null
}

private fun Opened.collectionLabel(): String = when (this) {
    is Opened.Ready -> peerId
    is Opened.Recovery -> peerId.orEmpty()
    else -> ""
}

@Composable
private fun IdentityCollectionRows(state: IdentityCollectionState, active: Opened.Ready, refresh: () -> Unit) {
    val snapshot = state.snapshot
    val records = remember(snapshot) {
        snapshot?.entries?.mapNotNull { entry -> entry.collectionTarget()?.let { it to entry } }?.toMap()
    }
    val entries = remember(records) {
        records?.map { (target, entry) -> LocalCollectionEntry(target, entry.collectionLabel()) }
    }
    if (snapshot == null && state.failed) {
        TextButton(refresh, Modifier.testTag("collection-refresh"), enabled = !state.busy) {
            Text(stringResource(R.string.collection_refresh))
        }
    } else {
        LocalCollectionView(state, entries, if (state.busy) null else refresh) { row, number ->
            records?.get(row.target)?.let { entry ->
                IdentityCollectionRow(entry, active.slot.id, number) { state.choose(entry) }
            }
        }
    }
}

@Composable
internal fun IdentityCollectionRow(entry: Opened, activeSlotId: String, number: Int, onRemove: () -> Unit) {
    val target = entry.collectionTarget() ?: return
    val peer = entry.collectionLabel()
    val isActive = target == activeSlotId
    Column(Modifier.fillMaxWidth().testTag("settings-identity")) {
        Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(12.dp), Alignment.CenterVertically) {
            Avatar(peer)
            Column(Modifier.weight(1f)) {
                Text(peer.ifEmpty { target }, fontFamily = FontFamily.Monospace)
                IdentityStatus(entry, isActive)
            }
        }
        if (removable(entry)) {
            val tag = if (isActive) "settings-remove" else "settings-remove-$number"
            TextButton(onClick = onRemove, Modifier.align(Alignment.End).testTag(tag)) {
                Text(stringResource(R.string.identity_remove))
            }
        }
    }
}

@Composable
private fun IdentityStatus(entry: Opened, isActive: Boolean) = when {
    entry is Opened.Recovery -> Caption(reasonText(entry.reason))
    isActive -> Caption(stringResource(R.string.settings_active_key), Modifier.testTag("settings-active"))
    else -> Caption(stringResource(R.string.settings_key_store))
}
