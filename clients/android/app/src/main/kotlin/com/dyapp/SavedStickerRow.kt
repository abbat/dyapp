package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackState

private const val PACK_ID_PREFIX_LENGTH = 8

internal fun stickerPackIdText(id: ByteArray): String = id.joinToString("") { "%02x".format(it) }

@Composable
internal fun SavedStickerRow(entry: SavedStickerPackEntry, state: SavedStickerPresentation) {
    val ready = entry.state as? SavedStickerPackState.Ready
    val name = ready?.name ?: stringResource(R.string.sticker_manager_unreadable)
    val id = stickerPackIdText(entry.packId)
    Column(Modifier.fillMaxWidth().testTag("saved-sticker-row-" + id)) {
        Text(name)
        Text(stringResource(R.string.sticker_manager_id, id.take(PACK_ID_PREFIX_LENGTH)))
        Text(stringResource(R.string.sticker_manager_bytes, entry.bytes.toString()))
        ready?.let { Text(stringResource(R.string.sticker_manager_count, it.stickers.toLong())) }
        val remove = { state.askRemove(entry) }
        TextButton(
            remove,
            Modifier.fillMaxWidth().testTag("saved-sticker-remove-" + id).applicationClick(remove),
            enabled = state.editable
        ) { Text(stringResource(R.string.sticker_manager_remove, name, id.take(PACK_ID_PREFIX_LENGTH))) }
    }
}
