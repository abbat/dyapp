package com.dyapp

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import uniffi.dyapp.ChatItem

@Composable
internal fun ChatThreadItem(item: ChatItem, me: String, form: ChatForm, state: uniffi.dyapp.ChatState?) {
    val writable = chatWritable(state)
    when (item) {
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
