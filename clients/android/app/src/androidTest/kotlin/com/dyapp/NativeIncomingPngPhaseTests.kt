package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.ChatState
import uniffi.dyapp.PhotoPurpose
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

/** Requires the suite-owned loopback host client; the phone remains a normal client. */
class NativeIncomingPngPhaseTests {
    @Test
    fun realPngKnownAndUnknownGeometryRevalidateHostileCacheAndDecodeExactPixels() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("incomingPngHost") == "1")
        TestPeer("incoming-png").use { receiver ->
            receiver.session.setMediaCache(true, receiver.session.mediaCache().limitBytes)
            HostPhotoPeer(receiver.peerId).use { host ->
                establish(receiver, host)
                val bytes = png()
                for ((width, height) in listOf(40u to 20u, 0u to 0u)) {
                    val result = host.photo(bytes, width, height)
                    val photo = awaitPhoto(receiver, host, result.photoHostBytes("id", 16))
                    assertArrayEquals(
                        MessageDigest.getInstance("SHA-256").digest(bytes),
                        result.photoHostBytes("sha256", 32)
                    )
                    assertPhoto(receiver, host, photo, bytes)
                }
            }
        }
    }

    private fun establish(receiver: TestPeer, host: HostPhotoPeer) {
        receiver.session.sendText(host.peerId, "incoming PNG invitation")
        var requestReceived = false
        PeerTransition("host PNG invitation").await({ requestReceived }) {
            requestReceived = host.receive().getBoolean("requestReceived")
        }
        host.text()
        receiver.awaitMessage(host.peerId, "host PNG reply") { it.text == "Android photo fixture" }
        assertEquals(ChatState.OPEN, receiver.session.state(host.peerId))
        assertTrue(host.receive().getBoolean("open"))
    }

    private fun png(): ByteArray {
        val bitmap = createBitmap(40, 20, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        return try {
            ByteArrayOutputStream().also {
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
            }.toByteArray()
        } finally {
            bitmap.recycle()
        }
    }

    private fun awaitPhoto(receiver: TestPeer, host: HostPhotoPeer, id: ByteArray): ChatPhoto {
        var found: ChatPhoto? = null
        PeerTransition("native PNG from host").await(
            ready = {
                found = receiver.session.chatPhotos(host.peerId)
                    .singleOrNull { it.author == host.peerId && it.id.contentEquals(id) }
                found != null
            },
            advance = {
                host.receive()
                receiver.session.receive()
            }
        )
        return checkNotNull(found)
    }

    private fun assertPhoto(receiver: TestPeer, host: HostPhotoPeer, photo: ChatPhoto, expected: ByteArray) {
        assertEquals("image/png", photo.mime)
        assertEquals(expected.size.toULong(), photo.size)
        fun original() = receiver.session.chatPhotoOriginal(host.peerId, photo.groupId, photo.author, photo.id)
        val before = receiver.originalCacheFiles()
        assertArrayEquals(expected, original())
        receiver.corruptCachedOriginal(before)
        val recovered = original()
        assertArrayEquals(expected, recovered)
        val bitmap = decodePhoto(recovered, PhotoPurpose.CHAT)
        try {
            assertEquals(40, bitmap.width)
            assertEquals(20, bitmap.height)
            assertEquals(Color.RED, bitmap.getPixel(bitmap.width / 2, bitmap.height / 2))
        } finally {
            bitmap.recycle()
        }
    }
}
