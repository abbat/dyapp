package com.dyapp

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.dyapp.ChatSticker

/** Only Delete is offered; native full-key selection still authorizes every confirmed operation. */
@Composable
internal fun StickerMessageMenu(sticker: ChatSticker, busy: () -> Boolean, bubble: @Composable () -> Unit) {
    val source = LocalChatStickerSource.current
    val delete = source?.delete
    if (source == null || delete == null) {
        bubble()
        return
    }
    key(source.owner, stickerKey(sticker)) {
        val state = remember { StickerDeletePresentation() }
        val scope = rememberCoroutineScope()
        DisposableEffect(state) { onDispose { state.retire() } }
        val confirm: (Any) -> Unit = { token ->
            if (scope.isActive && !busy()) {
                scope.launch(start = CoroutineStart.UNDISPATCHED) { state.confirm(token) { delete(sticker) } }
            }
        }
        Column {
            StickerOwnMenu(state, busy, bubble)
            StickerDeleteStatus(state)
        }
        StickerDeleteDialog(sticker, state, confirm, busy())
    }
}

@Composable
private fun StickerOwnMenu(state: StickerDeletePresentation, busy: () -> Boolean, bubble: @Composable () -> Unit) {
    var open by remember { mutableStateOf(false) }
    val label = stringResource(R.string.msg_menu)
    Box(
        Modifier.testTag("own-sticker-menu-target")
            .pointerInput(Unit) { detectTapGestures(onLongPress = { open = true }) }
            .semantics {
                onLongClick(label) {
                    open = true
                    true
                }
            }
    ) {
        bubble()
        DropdownMenu(open, { open = false }, Modifier.testTag("sticker-message-menu")) {
            val ask = {
                open = false
                if (!busy()) state.ask()
            }
            DropdownMenuItem(
                text = { Text(stringResource(R.string.chat_delete)) },
                onClick = ask,
                Modifier.testTag("sticker-message-delete").applicationClick(ask),
                enabled = state.editable && !busy()
            )
        }
    }
}
