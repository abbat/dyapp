package com.dyapp

import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatSubmission

internal fun stickerDeleteCalls(
    opened: Opened.Ready,
    peer: String,
    changed: () -> Unit
): (ChatSticker) -> ChatSubmission =
    { sticker ->
        RustBridge.withChats(opened) {
            it.submitDeleteSticker(peer, sticker.groupId, sticker.author, sticker.id)
        }.also { runCatching(changed) } // Keep commitment even if refreshing the visible history fails.
    }
