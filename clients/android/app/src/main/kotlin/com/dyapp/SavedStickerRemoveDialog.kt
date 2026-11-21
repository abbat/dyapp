package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import uniffi.dyapp.SavedStickerPackEntry

@Composable
internal fun SavedStickerRemoveDialog(
    state: SavedStickerPresentation,
    entry: SavedStickerPackEntry,
    confirm: () -> Unit
) {
    AlertDialog(
        onDismissRequest = state::cancelRemove,
        modifier = Modifier.testTag("saved-sticker-remove-dialog"),
        title = { Text(stringResource(R.string.sticker_manager_remove_title)) },
        text = { SavedStickerRemoveBody(entry) },
        confirmButton = {
            TextButton(
                confirm,
                Modifier.testTag("saved-sticker-confirm").applicationClick(confirm),
                enabled = !state.busy
            ) { Text(stringResource(R.string.sticker_manager_confirm)) }
        },
        dismissButton = {
            TextButton(state::cancelRemove, Modifier.testTag("saved-sticker-cancel"), enabled = !state.busy) {
                Text(stringResource(R.string.search_cancel))
            }
        }
    )
}

@Composable
private fun SavedStickerRemoveBody(entry: SavedStickerPackEntry) {
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(stringResource(R.string.sticker_manager_remove_body))
        SelectionContainer {
            Text(
                stickerPackIdText(entry.packId),
                Modifier.testTag("saved-sticker-remove-id"),
                fontFamily = FontFamily.Monospace
            )
        }
    }
}
