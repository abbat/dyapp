package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatStickerImage

@Composable
internal fun ChatStickerBubble(sticker: ChatSticker) {
    val source = LocalChatStickerSource.current
    key(source?.owner, stickerKey(sticker)) {
        val state = remember { ChatStickerState() }
        LaunchedEffect(NetworkType.version, source?.revision, LocalSavedStickerRevision.current) {
            if (source != null && state.image == null) state.load(false) { source.read(sticker, it) }
        }
        Column(Modifier.testTag("chat-sticker")) {
            val label = stringResource(R.string.chat_sticker_fallback, sticker.emoji, sticker.packName)
            val image = state.image
            if (image == null) {
                Text(label)
            } else {
                Image(
                    image,
                    label,
                    Modifier.widthIn(max = 192.dp).heightIn(max = 192.dp)
                        .aspectRatio(image.width.toFloat() / image.height)
                        .testTag("chat-sticker-image")
                )
            }
            if (source != null) StickerImageControls(state) { source.read(sticker, it) }
            val manage = LocalSavedStickerManager.current
            if (manage != null) source?.pack?.invoke(sticker)?.let { StickerPackControls(it, manage) }
        }
    }
}

@Composable
private fun StickerImageControls(state: ChatStickerState, read: (Boolean) -> ChatStickerImage) {
    if (state.image != null) return
    val scope = rememberCoroutineScope()
    Text(stringResource(stickerImageStatus(state)), Modifier.testTag("chat-sticker-status"))
    val action: () -> Unit = {
        if (scope.isActive) scope.launch { state.load(true, read) }
    }
    TextButton(
        action,
        Modifier.testTag("chat-sticker-load").applicationClick(action),
        enabled = !state.loading && (state.outcome != null || state.failed)
    ) { Text(stringResource(R.string.sticker_image_load)) }
}

private fun stickerImageStatus(state: ChatStickerState): Int = when {
    state.loading -> R.string.sticker_image_loading
    state.failed -> R.string.sticker_image_failed
    state.outcome == ChatStickerImage.OnTap -> R.string.sticker_image_tap
    state.outcome == ChatStickerImage.Unavailable -> R.string.sticker_image_unavailable
    else -> R.string.sticker_image_loading
}
