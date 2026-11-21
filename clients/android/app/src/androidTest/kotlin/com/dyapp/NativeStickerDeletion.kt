package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.CoreException

/** Reuse the suite-owned real peer pair; native receive completion advances terminal deletion. */
internal class NativeStickerDeletion(
    private val owner: TestPeer,
    private val peer: TestPeer,
    private val selected: ChatSticker
) {
    fun delete() {
        assertTrue(present(owner, peer.peerId))
        assertTrue(present(peer, owner.peerId))
        assertThrows(CoreException::class.java) { submit(peer, owner.peerId) }
        val result = submit(owner, peer.peerId)
        val operation = when (result) {
            is ChatSubmission.Accepted -> result.id
            is ChatSubmission.Queued -> result.id
        }
        assertEquals(16, operation.size)
        assertFalse(present(owner, peer.peerId))
        PeerTransition("native own sticker deletion").await(
            ready = { !present(peer, owner.peerId) },
            advance = {
                owner.session.receive()
                peer.session.receive()
            }
        )
        assertThrows(CoreException::class.java) { submit(owner, peer.peerId) }
        // The original sticker in the opposite direction and both saved packs remain.
        assertTrue(peer.session.chatItems(owner.peerId).filterIsInstance<ChatItem.Sticker>().isNotEmpty())
        assertTrue(owner.session.savedStickerPacks(null).entries.isNotEmpty())
        assertTrue(peer.session.savedStickerPacks(null).entries.isNotEmpty())
    }

    private fun submit(viewer: TestPeer, target: String) = viewer.session.submitDeleteSticker(
        target,
        selected.groupId,
        selected.author,
        selected.id
    )

    private fun present(viewer: TestPeer, target: String): Boolean =
        viewer.session.chatItems(target).filterIsInstance<ChatItem.Sticker>().any {
            it.sticker.author == selected.author &&
                it.sticker.groupId.contentEquals(selected.groupId) && it.sticker.id.contentEquals(selected.id)
        }
}
