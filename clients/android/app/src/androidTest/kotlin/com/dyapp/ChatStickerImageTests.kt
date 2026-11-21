package com.dyapp

import android.graphics.Color
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.core.graphics.get
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatStickerFrame
import uniffi.dyapp.ChatStickerImage
import java.util.concurrent.atomic.AtomicInteger

class ChatStickerImageTests {
    @get:Rule
    val compose = createComposeRule()

    private fun sticker() = ChatSticker(
        groupId = byteArrayOf(1), author = "sender", id = ByteArray(16),
        hlc = 1uL, sentAtMs = 1uL, packId = ByteArray(32), sha256 = ByteArray(32),
        emoji = ":)", packName = "Pack", unread = false, delivery = null
    )

    private fun frame() = ChatStickerFrame(
        width = 2u,
        height = 1u,
        pixels = byteArrayOf(-1, 0, 0, -1, 0, -1, 0, -128)
    )

    private fun show(source: () -> ChatStickerSource) {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalChatStickerSource provides source()) { ChatStickerBubble(sticker()) }
            }
        }
    }

    private fun status(resource: Int) {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(hasTestTag("chat-sticker-status") and hasText(context.getString(resource)))
    }

    @Test
    fun straightAlphaPixelsPreserveChannelsAndPremultipliedTransparency() {
        val bitmap = stickerBitmap(frame()).asAndroidBitmap()
        assertEquals(2, bitmap.width)
        assertEquals(1, bitmap.height)
        assertEquals(Color.RED, bitmap[0, 0])
        assertEquals(Color.argb(128, 0, 255, 0), bitmap[1, 0])
    }

    @Test
    fun nativeFramesKeepTheirAspectAndAccessibleMetadata() {
        val source = ChatStickerSource("owner") { _, manual ->
            check(!manual)
            ChatStickerImage.Ready(frame())
        }
        show { source }
        val image = compose.awaitNode(hasTestTag("chat-sticker-image"))
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        image.assertContentDescriptionEquals(context.getString(R.string.chat_sticker_fallback, ":)", "Pack"))
        val node = image.fetchSemanticsNode()
        assertEquals(2f, node.boundsInRoot.width / node.boundsInRoot.height, 0.01f)
        compose.onNodeWithTag("chat-sticker-load").assertDoesNotExist()
    }

    @Test
    fun repeatedManualRequestsAndRetiredCallbacksCannotCrossOwners() {
        val gate = StickerImageReadGate(frame())
        var owner by mutableStateOf("first")
        val second = ChatStickerSource("second") { _, manual ->
            check(!manual) { "retired action reached the successor" }
            ChatStickerImage.Unavailable
        }
        try {
            show { if (owner == "first") gate.source else second }
            status(R.string.sticker_image_tap)
            val action = compose.awaitNode(hasTestTag("chat-sticker-load") and isEnabled())
                .fetchSemanticsNode()
                .config[applicationClickKey]
            compose.runOnIdle { repeat(2) { action() } }
            compose.waitUntil(10_000) { gate.started.isDone }
            assertEquals(1, gate.calls.get())
            compose.runOnIdle { owner = "second" }
            status(R.string.sticker_image_unavailable)
            compose.runOnIdle { action() }
            gate.finish.complete(Unit)
            compose.waitUntil(10_000) { gate.returned.isDone }
            compose.waitForIdle()
            compose.onNodeWithTag("chat-sticker-image").assertDoesNotExist()
            assertEquals(1, gate.calls.get())
        } finally { gate.finish.complete(Unit) }
    }

    @Test
    fun failedReadKeepsTheFallbackAndManualRetryCanLoad() {
        val attempts = AtomicInteger()
        val source = ChatStickerSource("owner") { _, manual ->
            if (attempts.incrementAndGet() == 1) error("controlled read failure")
            check(manual)
            ChatStickerImage.Ready(frame())
        }
        show { source }
        status(R.string.sticker_image_failed)
        compose.awaitNode(hasTestTag("chat-sticker-load") and isEnabled()).performClick()
        compose.awaitNode(hasTestTag("chat-sticker-image"))
        assertEquals(2, attempts.get())
    }
}
