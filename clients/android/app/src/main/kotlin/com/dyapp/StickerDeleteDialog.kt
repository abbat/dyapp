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
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatSubmission

@Composable
internal fun StickerDeleteDialog(
    sticker: ChatSticker,
    state: StickerDeletePresentation,
    confirm: (Any) -> Unit,
    blocked: Boolean
) {
    val token = state.pending ?: return
    val action = { confirm(token) }
    AlertDialog(
        onDismissRequest = { state.cancel(token) },
        modifier = Modifier.testTag("sticker-delete-dialog"),
        title = { Text(stringResource(R.string.delete_title)) },
        text = { StickerDeleteExplanation(sticker, state) },
        confirmButton = {
            TextButton(
                action,
                Modifier.testTag("sticker-delete-confirm").applicationClick(action),
                enabled = state.editable && !blocked
            ) { Text(stringResource(R.string.chat_delete)) }
        },
        dismissButton = {
            TextButton({ state.cancel(token) }, Modifier.testTag("sticker-delete-cancel")) {
                Text(stringResource(if (state.busy) R.string.sticker_manager_close else R.string.remove_cancel))
            }
        }
    )
}

@Composable
private fun StickerDeleteExplanation(sticker: ChatSticker, state: StickerDeletePresentation) {
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(stringResource(R.string.chat_sticker_fallback, sticker.emoji, sticker.packName))
        Text(stringResource(R.string.delete_text))
        if (state.busy) Text(stringResource(R.string.sticker_delete_pending))
        if (state.failed) Text(stringResource(R.string.chat_delete_failed))
    }
}

@Composable
internal fun StickerDeleteStatus(state: StickerDeletePresentation) {
    if (state.failed && state.pending == null) Text(stringResource(R.string.chat_delete_failed))
    state.submitted?.let {
        val text = if (it is ChatSubmission.Queued) R.string.chat_delete_queued else R.string.sticker_delete_applied
        Text(stringResource(text), Modifier.testTag("sticker-delete-status"))
    }
}
