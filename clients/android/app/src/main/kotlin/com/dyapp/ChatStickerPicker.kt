package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.isActive

@Composable
internal fun ChatStickerPicker(enabled: Boolean) {
    val source = LocalChatStickerSource.current ?: return
    val calls = source.picker ?: return
    key(source.owner) { StickerPickerHost(calls, enabled) }
}

@Composable
private fun StickerPickerHost(calls: StickerPickerCalls, enabled: Boolean) {
    val manage = LocalSavedStickerManager.current ?: return
    val revision = LocalSavedStickerRevision.current
    val navigation = remember { StickerPickerNavigation() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(revision) { navigation.resume(revision, enabled) }
    TextButton(
        { if (scope.isActive) navigation.open(enabled) },
        Modifier.testTag("chat-stickers"),
        enabled = enabled && !navigation.waitingForManager
    ) { Text(stringResource(R.string.sticker_picker_title)) }
    navigation.opening?.let { token ->
        key(token) {
            StickerPickerOpening(calls, { navigation.close(token) }) {
                if (scope.isActive && navigation.manage(token, revision)) manage()
            }
        }
    }
}

@Composable
private fun StickerPickerOpening(calls: StickerPickerCalls, close: () -> Unit, manage: () -> Unit) {
    val state = remember { StickerPickerPresentation() }
    val scope = rememberCoroutineScope()
    val actions = remember { StickerPickerActions(state, calls, scope, close, manage) }
    DisposableEffect(state) { onDispose { state.retire() } }
    LaunchedEffect(state) { state.refresh(calls) }
    LaunchedEffect(state.submitted) { if (state.submitted != null) actions.close() }
    StickerPickerDialog(state, actions)
}
