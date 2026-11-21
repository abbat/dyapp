package com.dyapp

import uniffi.dyapp.ChatStickerFrame
import uniffi.dyapp.ChatStickerImage
import java.util.concurrent.CompletableFuture
import java.util.concurrent.atomic.AtomicInteger

/** A blocking native-like read survives presentation cancellation until explicit completion. */
internal class StickerImageReadGate(frame: ChatStickerFrame) {
    val started = CompletableFuture<Unit>()
    val finish = CompletableFuture<Unit>()
    val returned = CompletableFuture<Unit>()
    val calls = AtomicInteger()
    val source = ChatStickerSource("first") { _, manual ->
        if (manual) {
            calls.incrementAndGet()
            started.complete(Unit)
            try {
                finish.get()
                ChatStickerImage.Ready(frame)
            } finally { returned.complete(Unit) }
        } else {
            ChatStickerImage.OnTap
        }
    }
}
