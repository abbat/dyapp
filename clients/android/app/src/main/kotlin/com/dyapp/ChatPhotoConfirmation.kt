package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun ChatPhotoConfirmation(state: ChatPhotoDraft) {
    val photo = state.photo
    val preview by produceState<androidx.compose.ui.graphics.ImageBitmap?>(null, photo) {
        value = withContext(Dispatchers.Default) {
            photo?.let { runCatching { thumbnail(it.thumbnail) }.getOrNull() }
        }
    }
    Column {
        preview?.let { Image(it, stringResource(R.string.chat_photo), Modifier.heightIn(max = 240.dp)) }
        Text(stringResource(R.string.chat_photo_confirm))
        state.status?.let { Text(stringResource(it.text)) }
    }
}
