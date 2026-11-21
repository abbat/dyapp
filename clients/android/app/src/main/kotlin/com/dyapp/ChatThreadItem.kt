package com.dyapp

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatMessage

@Composable
internal fun ChatThreadItem(item: ChatItem, me: String, form: ChatForm, state: uniffi.dyapp.ChatState?) {
    when (item) {
        is ChatItem.Sticker -> {
            val alignment = if (item.sticker.author == me) Alignment.CenterEnd else Alignment.CenterStart
            Box(Modifier.fillMaxWidth(), alignment) {
                if (item.sticker.author == me) {
                    StickerMessageMenu(item.sticker, { form.busy }) { ChatStickerBubble(item.sticker) }
                } else {
                    ChatStickerBubble(item.sticker)
                }
            }
        }
        is ChatItem.Photo -> {
            ChatPhotoBubble(item.photo, item.photo.author != me && state != uniffi.dyapp.ChatState.OPEN)
        }
        is ChatItem.Text -> {
            ChatTextItem(item.message, me, form, state)
        }
    }
}

@Composable
private fun ChatTextItem(message: ChatMessage, me: String, form: ChatForm, state: uniffi.dyapp.ChatState?) {
    val own = message.author == me
    Box(Modifier.fillMaxWidth(), if (own) Alignment.CenterEnd else Alignment.CenterStart) {
        if (changeable(message, me) || hasForwardAction(message)) {
            MessageMenu(message, form, chatWritable(state)) { ChatBubble(message, own, it) }
        } else {
            ChatBubble(message, own, Modifier)
        }
    }
}
