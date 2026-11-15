package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.CoreException
import uniffi.dyapp.PhotoPurpose
import java.io.ByteArrayOutputStream

/** Real native clients of the suite-owned external node; no Activity or embedded Android node. */
class NativePhotoExchangeTests {
    @Test
    fun receivedJpegUsesItsFullSelectorAndBlockClearRefuseTheCachedOriginal() {
        TestPeer("photo-sender").use { sender ->
            TestPeer("photo-receiver").use { receiver ->
                establish(sender, receiver)
                val prepared = prepared()
                val outcome = sender.session.submitChatPhoto(
                    receiver.peerId,
                    prepared.full,
                    prepared.thumbnail,
                    prepared.width,
                    prepared.height
                )
                val id = when (outcome) {
                    is ChatSubmission.Accepted -> outcome.id
                    is ChatSubmission.Queued -> outcome.id
                }
                val photo = awaitPhoto(receiver, sender, id)
                assertReceived(receiver, sender, photo, prepared)
            }
        }
    }

    private fun assertReceived(receiver: TestPeer, sender: TestPeer, photo: ChatPhoto, prepared: PreparedPhoto) {
        assertEquals("image/jpeg", photo.mime)
        assertEquals(prepared.full.size.toULong(), photo.size)
        val projected = receiver.session.chatItems(sender.peerId).last() as ChatItem.Photo
        assertArrayEquals(photo.groupId, projected.photo.groupId)
        assertArrayEquals(photo.id, projected.photo.id)
        assertDecoded(prepared, original(receiver, sender, photo))
        refused(receiver, sender, photo.copy(groupId = different(photo.groupId)), "notFound")
        refused(receiver, sender, photo.copy(author = receiver.peerId), "notFound")
        refused(receiver, sender, photo.copy(id = different(photo.id)), "notFound")
        receiver.session.block(sender.peerId)
        assertTrue(sender.peerId in receiver.session.blockedPeers())
        refused(receiver, sender, photo, "blocked")
        receiver.session.unblock(sender.peerId)
        assertArrayEquals(prepared.full, original(receiver, sender, photo))
        assertTrue(receiver.session.clearChat(sender.peerId) > 0u)
        assertTrue(receiver.session.chatPhotos(sender.peerId).isEmpty())
        refused(receiver, sender, photo, "notFound")
    }

    private fun establish(sender: TestPeer, receiver: TestPeer) {
        sender.session.sendText(receiver.peerId, "photo anchor")
        receiver.awaitMessage(sender.peerId, "photo invitation") { it.text == "photo anchor" }
        receiver.session.sendText(sender.peerId, "photo reply")
        sender.awaitMessage(receiver.peerId, "photo reply") { it.text == "photo reply" }
        assertEquals(ChatState.OPEN, sender.session.state(receiver.peerId))
        assertEquals(ChatState.OPEN, receiver.session.state(sender.peerId))
    }

    private fun prepared(): PreparedPhoto {
        val bitmap = createBitmap(40, 20).apply { eraseColor(Color.RED) }
        return try {
            val bytes = ByteArrayOutputStream().also { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            preparePhoto(bytes.toByteArray(), PhotoPurpose.CHAT)
        } finally {
            bitmap.recycle()
        }
    }

    private fun awaitPhoto(receiver: TestPeer, sender: TestPeer, id: ByteArray): ChatPhoto {
        var found: ChatPhoto? = null
        PeerTransition("native JPEG from ${sender.peerId}").await(
            ready = {
                found = receiver.session.chatPhotos(sender.peerId)
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

    private fun original(receiver: TestPeer, sender: TestPeer, photo: ChatPhoto): ByteArray =
        receiver.session.chatPhotoOriginal(sender.peerId, photo.groupId, photo.author, photo.id)

    private fun assertDecoded(prepared: PreparedPhoto, bytes: ByteArray) {
        assertArrayEquals(prepared.full, bytes)
        val bitmap = decodePhoto(bytes, PhotoPurpose.CHAT)
        try {
            assertEquals(prepared.width, bitmap.width.toUInt())
            assertEquals(prepared.height, bitmap.height.toUInt())
        } finally {
            bitmap.recycle()
        }
    }

    private fun different(bytes: ByteArray): ByteArray {
        check(bytes.isNotEmpty())
        return bytes.copyOf().apply { this[0] = (this[0].toInt() xor 1).toByte() }
    }

    private fun refused(receiver: TestPeer, sender: TestPeer, photo: ChatPhoto, reason: String) {
        val failure = assertThrows(CoreException::class.java) { original(receiver, sender, photo) }
        assertTrue(failure.toString(), failure.message.orEmpty().contains(reason))
    }
}
