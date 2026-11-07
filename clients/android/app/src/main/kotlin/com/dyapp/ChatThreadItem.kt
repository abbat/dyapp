package com.dyapp

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatItem

@Composable
internal fun ChatThreadItem(item: ChatItem, me: String, form: ChatForm, state: uniffi.dyapp.ChatState?) {
    val writable = chatWritable(state)
    when (item) {
        is ChatItem.Sticker -> {
            val alignment = if (item.sticker.author == me) Alignment.CenterEnd else Alignment.CenterStart
            Box(Modifier.fillMaxWidth(), alignment) {
                Text(stringResource(R.string.chat_sticker_fallback, item.sticker.emoji, item.sticker.packName))
            }
        }
        is ChatItem.Photo -> {
            ChatPhotoBubble(item.photo, item.photo.author != me && state != uniffi.dyapp.ChatState.OPEN)
        }
        is ChatItem.Text -> {
            val message = item.message
            val own = message.author == me
            Box(Modifier.fillMaxWidth(), if (own) Alignment.CenterEnd else Alignment.CenterStart) {
                if (changeable(message, me)) {
                    MessageMenu(message, form, writable) { ChatBubble(message, true, it) }
                } else {
                    ChatBubble(message, own, Modifier)
                }
            }
        }
    }
}
