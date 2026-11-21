package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ChatStickerFrame

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun StickerPickerChoices(
    shown: StickerPickerView,
    data: StickerPickerData.Choices,
    busy: Boolean,
    actions: StickerPickerActions
) {
    if (data.entries.isEmpty()) Text(stringResource(R.string.sticker_picker_empty))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        data.entries.forEach { choice ->
            StickerChoiceButton(choice, busy) { actions.send(shown, choice) }
        }
    }
    data.next?.let { after ->
        val query = shown.query as? StickerPickerQuery.Choices
        TextButton(
            { actions.navigate(StickerPickerQuery.Choices(query?.packId, after), shown) },
            Modifier.testTag("sticker-picker-next"),
            enabled = !busy && query != null
        ) { Text(stringResource(R.string.sticker_manager_next)) }
    }
}

@Composable
private fun StickerChoiceButton(choice: StickerPickerChoice, busy: Boolean, send: () -> Unit) {
    val entry = choice.sticker
    val bitmap = remember(choice) { stickerBitmap(ChatStickerFrame(entry.width, entry.height, entry.pixels)) }
    val label = stringResource(R.string.chat_sticker_fallback, entry.emoji, choice.name)
    val key = stickerPackIdText(choice.packId) + "-" + entry.index
    TextButton(
        send,
        Modifier.testTag("sticker-picker-choice-" + key).applicationClick(send)
            .semantics { contentDescription = label },
        enabled = !busy
    ) {
        Column {
            Image(
                bitmap,
                null,
                Modifier.width(entry.width.toFloat().dp).height(entry.height.toFloat().dp)
                    .testTag("sticker-picker-image-" + key)
            )
            Text(label)
        }
    }
}
