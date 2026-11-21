package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

/** One bounded core page; closing the view does not interrupt already-admitted native work. */
@Composable
internal fun SavedStickerManagerView(state: SavedStickerPresentation, actions: SavedStickerActions) {
    AlertDialog(
        onDismissRequest = actions.close,
        modifier = Modifier.testTag("saved-sticker-manager"),
        title = { Text(stringResource(R.string.sticker_manager_title)) },
        text = { SavedStickerBody(state, actions) },
        confirmButton = {},
        dismissButton = {
            TextButton(actions.close, Modifier.testTag("saved-sticker-close")) {
                Text(stringResource(R.string.sticker_manager_close))
            }
        }
    )
    state.pending?.let { confirmation ->
        SavedStickerRemoveDialog(state, confirmation.entry) { actions.confirm(confirmation) }
    }
}

@Composable
private fun SavedStickerBody(state: SavedStickerPresentation, actions: SavedStickerActions) {
    val page = state.page
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(stringResource(R.string.sticker_manager_hint))
        SavedStickerNotices(state)
        SavedStickerToolbar(state, actions)
        page?.let {
            Text(
                stringResource(
                    R.string.sticker_manager_usage,
                    it.usedBytes.toString(),
                    it.unrecognizedFiles.toString()
                )
            )
            if (it.entries.isEmpty()) Text(stringResource(R.string.sticker_manager_empty))
            it.entries.forEach { entry -> SavedStickerRow(entry, state) }
        }
    }
}

@Composable
private fun SavedStickerNotices(state: SavedStickerPresentation) {
    if (state.busy) Text(stringResource(R.string.sticker_manager_loading))
    if (state.readFailed) Text(stringResource(R.string.sticker_manager_read_failed))
    if (state.operationFailed) Text(stringResource(R.string.sticker_manager_action_failed))
    state.imported?.let {
        Text(stringResource(if (it.added) R.string.sticker_manager_added else R.string.sticker_manager_already))
    }
}

@Composable
private fun SavedStickerToolbar(state: SavedStickerPresentation, actions: SavedStickerActions) {
    val shown = state.page
    val refresh = { actions.navigate(null, shown) }
    Column {
        TextButton(
            actions.pick,
            Modifier.fillMaxWidth().testTag("saved-sticker-import"),
            enabled = state.editable
        ) { Text(stringResource(R.string.sticker_manager_import)) }
        TextButton(
            refresh,
            Modifier.fillMaxWidth().testTag("saved-sticker-refresh").applicationClick(refresh),
            enabled = !state.busy && state.pending == null && state.pickerTicket == null
        ) { Text(stringResource(R.string.sticker_manager_refresh)) }
        shown?.next?.let { after ->
            val next = { actions.navigate(after, shown) }
            TextButton(
                next,
                Modifier.fillMaxWidth().testTag("saved-sticker-next").applicationClick(next),
                enabled = state.editable
            ) { Text(stringResource(R.string.sticker_manager_next)) }
        }
    }
}
