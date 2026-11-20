package com.dyapp

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import androidx.core.graphics.createBitmap
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ChatState
import uniffi.dyapp.CoreException
import uniffi.dyapp.PhotoPurpose
import java.io.ByteArrayOutputStream
import java.io.File
import java.security.MessageDigest

/** Checks borrow a native session; the caller closes its handle after completion. */
internal class IncomingPngChecks(private val session: ChatSession, private val directory: File) {
    fun run(host: HostPhotoPeer): ChatPhoto {
        session.setMediaCache(true, session.mediaCache().limitBytes)
        establish(host)
        val bytes = png()
        var saved: ChatPhoto? = null
        for ((width, height) in listOf(40u to 20u, 0u to 0u)) {
            val before = originalCacheFiles()
            val result = host.photo(bytes, width, height)
            val photo = awaitPhoto(host, result.photoHostBytes("id", 16))
            assertArrayEquals(
                MessageDigest.getInstance("SHA-256").digest(bytes),
                result.photoHostBytes("sha256", 32)
            )
            assertPhoto(host, photo, bytes, before)
            saved = photo
        }
        rejectPhoto(host, bytes, 41u to 20u)
        rejectOversizedGeometry(host, bytes)
        return checkNotNull(saved)
    }

    private fun rejectPhoto(host: HostPhotoPeer, bytes: ByteArray, geometry: Pair<UInt, UInt>) {
        val before = originalCacheFiles()
        val result = host.photo(bytes, geometry.first, geometry.second)
        val photo = awaitPhoto(host, result.photoHostBytes("id", 16))
        val failure = assertThrows(CoreException::class.java) {
            session.chatPhotoOriginal(host.peerId, photo.groupId, photo.author, photo.id)
        }
        assertTrue(failure.toString(), failure.message.orEmpty().contains("corrupted"))
        assertEquals(before, originalCacheFiles())
    }

    private fun rejectOversizedGeometry(host: HostPhotoPeer, bytes: ByteArray) {
        val oversized = oversizedPng(bytes)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(oversized, 0, oversized.size, bounds)
        assertEquals(OVERSIZED_PNG_WIDTH, bounds.outWidth)
        assertEquals(OVERSIZED_PNG_HEIGHT, bounds.outHeight)
        // The refusal assertion never passes this hostile frame to the allocating decoder.
        rejectPhoto(host, oversized, 0u to 0u)
    }

    private fun establish(host: HostPhotoPeer) {
        session.sendText(host.peerId, "incoming PNG invitation")
        var requestReceived = false
        PeerTransition("host PNG invitation").await({ requestReceived }) {
            requestReceived = host.receive().getBoolean("requestReceived")
        }
        host.text()
        PeerTransition("host PNG reply").await(
            ready = {
                session.chatHistory(host.peerId).messages.any {
                    it.author == host.peerId && it.text == "Android photo fixture"
                }
            },
            advance = { session.receive() }
        )
        assertEquals(ChatState.OPEN, session.state(host.peerId))
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

    private fun awaitPhoto(host: HostPhotoPeer, id: ByteArray): ChatPhoto {
        var found: ChatPhoto? = null
        PeerTransition("native PNG from host").await(
            ready = {
                found = session.chatPhotos(host.peerId)
                    .singleOrNull { it.author == host.peerId && it.id.contentEquals(id) }
                found != null
            },
            advance = {
                host.receive()
                session.receive()
            }
        )
        return checkNotNull(found)
    }

    private fun originalCacheFiles(): Set<File> =
        File(directory, "originals").listFiles().orEmpty().filter { it.isFile }.toSet()

    private fun corruptCachedOriginal(before: Set<File>) {
        val created = (originalCacheFiles() - before).single()
        created.writeBytes("invalid PNG cache".toByteArray())
    }

    private fun assertPhoto(host: HostPhotoPeer, photo: ChatPhoto, expected: ByteArray, before: Set<File>) {
        assertEquals("image/png", photo.mime)
        assertEquals(expected.size.toULong(), photo.size)
        fun original() = session.chatPhotoOriginal(host.peerId, photo.groupId, photo.author, photo.id)
        assertArrayEquals(expected, original())
        corruptCachedOriginal(before)
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
