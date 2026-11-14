package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.StickerPackSaveAction
import uniffi.dyapp.StickerPackSaveAttempt
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus

/** One explicit save transition; each receive return is the readiness signal for the next read. */
internal class NativeStickerSave(
    private val receiver: TestPeer,
    private val sender: TestPeer,
    private val sticker: ChatSticker,
) {
    fun save() {
        val receiverRows = receiver.session.chatItems(sender.peerId).size
        val senderRows = sender.session.chatItems(receiver.peerId).size
        val waiting = request()
        assertEquals(StickerPackSaveState.WAITING, waiting.state)
        assertEquals(16, checkNotNull(waiting.generation).size)
        assertArrayEquals(waiting.generation, request().generation)
        var refreshed = false
        PeerTransition("explicit native sticker pack save").await(
            ready = { status().state == StickerPackSaveState.SAVED },
            advance = {
                refreshed = receiver.session.receive() > 0u || refreshed // Deliver/retry, possibly finish.
                sender.session.receive() // Authorize and answer it through core.
                refreshed = receiver.session.receive() > 0u || refreshed
            }
        )
        assertTrue("completed save contributes to refresh", refreshed)
        assertEquals(receiverRows, receiver.session.chatItems(sender.peerId).size)
        assertEquals(senderRows, sender.session.chatItems(receiver.peerId).size)
    }

    private fun request(): StickerPackSaveStatus {
        val attempt = receiver.session.changeStickerPackRequest(
            sender.peerId,
            sticker.groupId,
            sticker.author,
            sticker.id,
            StickerPackSaveAction.Request
        )
        check(attempt is StickerPackSaveAttempt.Ready) { "native save status unavailable: $attempt" }
        return attempt.status
    }

    private fun status() = receiver.session.chatStickerPackStatus(
        sender.peerId,
        sticker.groupId,
        sticker.author,
        sticker.id
    )
}
