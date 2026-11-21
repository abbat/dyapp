package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatStickerImage

/** The owner includes the actual opened identity and peer, never only the displayed Peer ID. */
internal class ChatStickerSource(
    val owner: Any,
    val revision: Int = 0,
    val pack: (ChatSticker) -> StickerPackTarget? = { null },
    val picker: StickerPickerCalls? = null,
    val delete: ((ChatSticker) -> uniffi.dyapp.ChatSubmission)? = null,
    val read: (ChatSticker, Boolean) -> ChatStickerImage
)

internal val LocalChatStickerSource = staticCompositionLocalOf<ChatStickerSource?> { null }

internal class ChatStickerState {
    var loading by mutableStateOf(false)
        private set
    var failed by mutableStateOf(false)
        private set
    var outcome by mutableStateOf<ChatStickerImage?>(null)
        private set
    var image by mutableStateOf<ImageBitmap?>(null)
        private set

    suspend fun load(manual: Boolean, read: (Boolean) -> ChatStickerImage) {
        if (loading || image != null) return
        loading = true
        failed = false
        try {
            val result = withContext(Dispatchers.IO) {
                runCatching { read(manual).let { it to (it as? ChatStickerImage.Ready)?.frame?.let(::stickerBitmap) } }
            }
            currentCoroutineContext().ensureActive()
            result.onSuccess { (status, pixels) ->
                outcome = status
                image = pixels
            }.onFailure { failed = true }
        } finally {
            loading = false
        }
    }
}

/** Full native selection plus immutable content; byte arrays need value keys across history reads. */
internal fun stickerKey(sticker: ChatSticker): List<Any> = listOf(
    sticker.groupId.toList(),
    sticker.author,
    sticker.id.toList(),
    sticker.packId.toList(),
    sticker.sha256.toList()
)
