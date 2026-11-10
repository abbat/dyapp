package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.PhotoPurpose

internal val LocalChatPhotoAuto = staticCompositionLocalOf<(() -> Boolean)?> { null }

internal val LocalChatPhotoLoader = staticCompositionLocalOf<((ChatPhoto) -> ByteArray)?> { null }

internal class ChatPhotoState {
    var revealed by mutableStateOf(false)
    var loading by mutableStateOf(false)
    var image by mutableStateOf<ImageBitmap?>(null)
    var error by mutableStateOf<Int?>(null)

    suspend fun load(photo: ChatPhoto, fetch: (ChatPhoto) -> ByteArray) {
        loading = true
        error = null
        val result = try {
            withContext(Dispatchers.IO) { runCatching { fullImage(fetch(photo), PhotoPurpose.CHAT) } }
        } finally {
            loading = false
        }
        result.onSuccess { image = it }.onFailure {
            error = if ("notFound" in it.toString()) R.string.chat_photo_gone else R.string.chat_photo_failed
        }
    }
}
