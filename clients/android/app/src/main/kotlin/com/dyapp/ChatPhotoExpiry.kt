package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.delay
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.chatPhotoHoursLeft

private const val EXPIRY_REFRESH_MS = 60_000L

@Composable
internal fun ChatPhotoExpiry(photo: ChatPhoto, downloaded: Boolean) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(photoKey(photo)) {
        while (true) {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { delay(EXPIRY_REFRESH_MS) }
            now = System.currentTimeMillis()
        }
    }
    if (downloaded) return
    chatPhotoHoursLeft(photo.sentAtMs, now.coerceAtLeast(0).toULong())?.let {
        Text(stringResource(R.string.chat_photo_hours, it.toString()))
    }
}
