package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.SavedStickerPackState

@Composable
internal fun StickerPickerPacks(
    shown: StickerPickerView,
    page: SavedStickerPackPage,
    busy: Boolean,
    actions: StickerPickerActions
) {
    TextButton(
        { actions.navigate(StickerPickerQuery.Choices(), shown) },
        Modifier.testTag("sticker-picker-recent"),
        enabled = !busy
    ) { Text(stringResource(R.string.sticker_picker_recent)) }
    if (page.entries.isEmpty()) Text(stringResource(R.string.sticker_picker_no_packs))
    page.entries.forEach { entry ->
        val ready = entry.state as? SavedStickerPackState.Ready
        TextButton(
            { actions.navigate(StickerPickerQuery.Choices(entry.packId), shown) },
            Modifier.fillMaxWidth().testTag("sticker-picker-pack-" + stickerPackIdText(entry.packId)),
            enabled = !busy && ready != null
        ) { Text(ready?.name ?: stringResource(R.string.sticker_manager_unreadable)) }
    }
    page.next?.let { after ->
        TextButton(
            { actions.navigate(StickerPickerQuery.Packs(after), shown) },
            Modifier.testTag("sticker-picker-packs-next"),
            enabled = !busy
        ) { Text(stringResource(R.string.sticker_manager_next)) }
    }
}
