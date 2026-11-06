package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatPhoto

internal val LocalChatItems = staticCompositionLocalOf<List<ChatItem>?> { null }

/** Metadata is safe to show before the user allows a stranger's preview or original. */
@Composable
internal fun ChatPhotoPlaceholder(photo: ChatPhoto) {
    Surface(Modifier.testTag("chat-photo-${photoKey(photo)}")) {
        Column(Modifier.padding(12.dp)) {
            Text(stringResource(R.string.chat_photo))
            Text(stringResource(R.string.chat_photo_metadata, photo.mime, photo.size.toString()))
        }
    }
}
