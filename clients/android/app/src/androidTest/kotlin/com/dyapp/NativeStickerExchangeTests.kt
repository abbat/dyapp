package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatMailNotice
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.SavedStickerFrame
import uniffi.dyapp.SavedStickerPackState
import uniffi.dyapp.StickerPickerEntry

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
                val sticker = awaitSticker(receiver, sender, id, stickerNoticeText())
                assertArrayEquals(imported.packId, sticker.packId)
                assertEquals("\uD83D\uDE3A", sticker.emoji)
                assertEquals("Android native cats", sticker.packName)
                assertOriginal(receiver, sticker, expected)
                NativeStickerSave(receiver, sender, sticker).save()
                val choice = assertSaved(receiver, sticker, expected)
                val reply = submittedId(receiver.session.submitSavedSticker(sender.peerId, sticker.packId, 0u))
                assertRecent(receiver, sticker, choice)
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
        sender.session.sendText(receiver.peerId, "older private words")
        receiver.awaitMessage(sender.peerId, "retained unread text") { it.text == "older private words" }
        val last = checkNotNull(receiver.session.chatRows().single { it.peerId == sender.peerId }.lastMessage)
        assertEquals("older private words", last.text)
        assertTrue(last.unread)
        receiver.session.setNotice(ChatNotice.TEXT)
    }

    private fun submittedId(value: ChatSubmission): ByteArray = when (value) {
        is ChatSubmission.Accepted -> value.id
        is ChatSubmission.Queued -> value.id // Completed receive passes retry these exact writes.
    }

    private fun awaitSticker(
        receiver: TestPeer,
        sender: TestPeer,
        id: ByteArray,
        expectedNotice: String? = null,
    ): ChatSticker {
        var found: ChatSticker? = null
        var admitted: ChatMailNotice? = null
        val label = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.notice_sticker)
        PeerTransition("native sticker from ${sender.peerId}").await(
            ready = {
                found = receiver.session.chatItems(sender.peerId).filterIsInstance<ChatItem.Sticker>()
                    .map { it.sticker }
                    .singleOrNull { it.author == sender.peerId && it.id.contentEquals(id) }
                found != null
            },
            advance = {
                sender.session.receive()
                receiver.session.receiveWithNotice(label).notice?.takeIf { it.peerId == sender.peerId }?.let {
                    admitted = it
                }
            }
        )
        if (expectedNotice != null) assertEquals(expectedNotice, checkNotNull(admitted).text)
        return checkNotNull(found)
    }

    private fun stickerNoticeText() = "\uD83D\uDE3A " +
        InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.notice_sticker)

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

    private fun assertSaved(receiver: TestPeer, sticker: ChatSticker, expected: SavedStickerFrame): StickerPickerEntry {
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
        val page = receiver.session.savedStickerPage(sticker.packId, 0u)
        assertArrayEquals(sticker.packId, page.packId)
        assertEquals(1u, page.total)
        assertEquals(null, page.next)
        return page.entries.single().also { choice ->
            assertEquals(0u, choice.index)
            assertEquals(sticker.emoji, choice.emoji)
            assertTrue(choice.width in 1u..96u && choice.height in 1u..96u)
            assertEquals(choice.width * choice.height * 4u, choice.pixels.size.toUInt())
        }
    }

    private fun assertRecent(receiver: TestPeer, sticker: ChatSticker, choice: StickerPickerEntry) {
        val page = receiver.session.recentStickerPage(0u)
        assertEquals(null, page.next)
        val entry = page.entries.single()
        assertArrayEquals(sticker.packId, entry.packId)
        assertEquals(sticker.packName, entry.packName)
        assertEquals(choice.index, entry.sticker.index)
        assertEquals(choice.emoji, entry.sticker.emoji)
        assertEquals(choice.width, entry.sticker.width)
        assertEquals(choice.height, entry.sticker.height)
        assertArrayEquals(choice.pixels, entry.sticker.pixels)
    }
}
