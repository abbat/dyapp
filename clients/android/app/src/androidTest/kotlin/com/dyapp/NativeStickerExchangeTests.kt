package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.SavedStickerFrame
import uniffi.dyapp.SavedStickerPackState

/** Native-only consumers of the suite-owned real node; no Activity or screen changes. */
class NativeStickerExchangeTests {
    @Test
    fun explicitPackSaveAndResendReachTheOtherNativeClientWithoutControlRows() {
        TestPeer("sticker-sender").use { sender ->
            TestPeer("sticker-receiver").use { receiver ->
                establish(sender, receiver)
                val imported = sender.session.importStickerPack(archive())
                assertTrue(imported.added)
                val expected = sender.session.savedStickerFrame(imported.packId, 0u)
                val id = submittedId(sender.session.submitSavedSticker(receiver.peerId, imported.packId, 0u))
                val sticker = awaitSticker(receiver, sender, id)
                assertArrayEquals(imported.packId, sticker.packId)
                assertEquals("\uD83D\uDE3A", sticker.emoji)
                assertEquals("Android native cats", sticker.packName)
                assertOriginal(receiver, sticker, expected)
                NativeStickerSave(receiver, sender, sticker).save()
                assertSaved(receiver, sticker, expected)
                val reply = submittedId(receiver.session.submitSavedSticker(sender.peerId, sticker.packId, 0u))
                val returned = awaitSticker(sender, receiver, reply)
                assertArrayEquals(sticker.packId, returned.packId)
                assertArrayEquals(sticker.sha256, returned.sha256)
                assertEquals(sticker.emoji, returned.emoji)
            }
        }
    }

    private fun archive(): ByteArray =
        // Deterministic USTAR: one manifest plus the repository's lossless-alpha.webp fuzz seed.
        InstrumentationRegistry.getInstrumentation().context.assets.open("sticker-pack.tar").use { it.readBytes() }

    private fun establish(sender: TestPeer, receiver: TestPeer) {
        sender.session.sendText(receiver.peerId, "sticker anchor")
        receiver.awaitMessage(sender.peerId, "sticker invitation") { it.text == "sticker anchor" }
        receiver.session.sendText(sender.peerId, "sticker reply")
        sender.awaitMessage(receiver.peerId, "sticker reply") { it.text == "sticker reply" }
        assertEquals(ChatState.OPEN, sender.session.state(receiver.peerId))
        assertEquals(ChatState.OPEN, receiver.session.state(sender.peerId))
    }

    private fun submittedId(value: ChatSubmission): ByteArray = when (value) {
        is ChatSubmission.Accepted -> value.id
        is ChatSubmission.Queued -> value.id // Completed receive passes retry these exact writes.
    }

    private fun awaitSticker(receiver: TestPeer, sender: TestPeer, id: ByteArray): ChatSticker {
        var found: ChatSticker? = null
        PeerTransition("native sticker from ${sender.peerId}").await(
            ready = {
                found = receiver.session.chatItems(sender.peerId).filterIsInstance<ChatItem.Sticker>()
                    .map { it.sticker }
                    .singleOrNull { it.author == sender.peerId && it.id.contentEquals(id) }
                found != null
            },
            advance = {
                sender.session.receive()
                receiver.session.receive()
            }
        )
        return checkNotNull(found)
    }

    private fun assertOriginal(receiver: TestPeer, sticker: ChatSticker, expected: SavedStickerFrame) {
        val frame = receiver.session.chatStickerFrame(
            sticker.author,
            sticker.groupId,
            sticker.author,
            sticker.id,
            true
        )
        assertEquals(expected.width, frame.width)
        assertEquals(expected.height, frame.height)
        assertArrayEquals(expected.pixels, frame.pixels)
    }

    private fun assertSaved(receiver: TestPeer, sticker: ChatSticker, expected: SavedStickerFrame) {
        val entry = receiver.session.savedStickerPacks(null).entries.single()
        assertArrayEquals(sticker.packId, entry.packId)
        val state = entry.state as SavedStickerPackState.Ready
        assertEquals("Android native cats", state.name)
        assertEquals(1u, state.stickers)
        val frame = receiver.session.savedStickerFrame(sticker.packId, 0u)
        assertArrayEquals(sticker.sha256, frame.sha256)
        assertEquals(sticker.emoji, frame.emoji)
        assertEquals(expected.width, frame.width)
        assertEquals(expected.height, frame.height)
        assertArrayEquals(expected.pixels, frame.pixels)
    }
}
