package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

@Composable
internal fun StickerPickerDialog(state: StickerPickerPresentation, actions: StickerPickerActions) {
    AlertDialog(
        onDismissRequest = actions::close,
        modifier = Modifier.testTag("sticker-picker"),
        title = { Text(stringResource(R.string.sticker_picker_title)) },
        text = { StickerPickerBody(state, actions) },
        confirmButton = {
            TextButton(actions::manage, Modifier.testTag("sticker-picker-manage")) {
                Text(stringResource(R.string.sticker_picker_manage))
            }
        },
        dismissButton = {
            TextButton(actions::close, Modifier.testTag("sticker-picker-cancel")) {
                Text(stringResource(R.string.search_cancel))
            }
        }
    )
}

@Composable
private fun StickerPickerBody(state: StickerPickerPresentation, actions: StickerPickerActions) {
    val shown = state.view
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(stringResource(R.string.sticker_picker_hint))
        if (state.busy) Text(stringResource(R.string.sticker_picker_busy))
        if (state.sendFailed) Text(stringResource(R.string.sticker_picker_send_failed))
        when (val data = shown?.data) {
            is StickerPickerData.Packs -> StickerPickerPacks(shown, data.page, state.busy, actions)
            is StickerPickerData.Choices -> StickerPickerChoices(shown, data, state.busy, actions)
            null -> if (shown != null) Text(stringResource(R.string.sticker_picker_read_failed))
        }
        TextButton(actions::refresh, Modifier.testTag("sticker-picker-refresh"), enabled = !state.busy) {
            Text(stringResource(R.string.sticker_picker_refresh))
        }
        if (shown != null && state.hasPrevious) {
            TextButton(
                { actions.back(shown) },
                Modifier.testTag("sticker-picker-back"),
                enabled = !state.busy
            ) { Text(stringResource(R.string.lock_back)) }
        }
    }
}
