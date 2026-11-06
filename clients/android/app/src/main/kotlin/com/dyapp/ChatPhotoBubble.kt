package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatPhoto

@Composable
internal fun ChatPhotoBubble(photo: ChatPhoto, stranger: Boolean) {
    val state = remember(photoKey(photo)) { ChatPhotoState() }
    var preview by remember(photoKey(photo)) { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(photoKey(photo)) {
        preview = withContext(Dispatchers.Default) { runCatching { thumbnail(photo.preview) }.getOrNull() }
    }
    ChatPhotoAuto(photo, state, stranger)
    Column(Modifier.padding(8.dp)) {
        ChatPhotoPlaceholder(photo)
        val concealed = stranger && !state.revealed
        val shown = state.image ?: preview
        shown?.let {
            val ratio = state.image?.let { full -> full.width.toFloat() / full.height } ?: photoAspect(photo)
            Image(it, stringResource(R.string.chat_photo),
                Modifier.fillMaxWidth().heightIn(max = 320.dp).aspectRatio(ratio)
                    .then(if (concealed) Modifier.blur(24.dp) else Modifier))
        }
        ChatPhotoExpiry(photo, state.image != null)
        ChatPhotoControls(photo, state, concealed)
    }
}

@Composable
private fun ChatPhotoAuto(photo: ChatPhoto, state: ChatPhotoState, stranger: Boolean) {
    val auto = LocalChatPhotoAuto.current
    val fetch = LocalChatPhotoLoader.current
    LaunchedEffect(photoKey(photo), NetworkType.version, stranger) {
        if (auto == null || fetch == null) return@LaunchedEffect
        if (!stranger && state.image == null && !state.loading) {
            val allowed = withContext(Dispatchers.IO) { runCatching { auto() }.getOrDefault(false) }
            if (allowed && !state.loading && state.image == null) state.load(photo, fetch)
        }
    }
}

@Composable
private fun ChatPhotoControls(photo: ChatPhoto, state: ChatPhotoState, concealed: Boolean) {
    val fetch = LocalChatPhotoLoader.current
    val scope = rememberCoroutineScope()
    state.error?.let { Text(stringResource(it)) }
    if (concealed) {
        TextButton({ state.revealed = true }) { Text(stringResource(R.string.chat_photo_show)) }
    } else if (state.image == null && fetch != null) {
        TextButton({
            if (!state.loading) {
                state.loading = true
                scope.launch { state.load(photo, fetch) }
            }
        }, enabled = !state.loading) {
            Text(stringResource(if (state.loading) R.string.view_loading else R.string.chat_photo_download))
        }
    }
}
