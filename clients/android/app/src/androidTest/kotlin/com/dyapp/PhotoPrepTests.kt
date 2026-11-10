package com.dyapp

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import androidx.core.graphics.createBitmap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.dyapp.PhotoPurpose
import uniffi.dyapp.checkPreview
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

class PhotoPrepTests {
    @Test
    fun opaqueAndPartialAlphaChannelsReachTheCoreAsStraightRgba() {
        for ((picked, expected) in listOf(
            Color.rgb(64, 32, 16) to Color.rgb(64, 32, 16),
            Color.argb(128, 64, 32, 16) to Color.rgb(159, 143, 135),
            Color.TRANSPARENT to Color.WHITE
        )) {
            val photo = preparePhoto(encoded(40, 20, picked, Bitmap.CompressFormat.PNG))
            val bitmap = requireNotNull(BitmapFactory.decodeByteArray(photo.full, 0, photo.full.size))
            try {
                assertRgb(expected, bitmap.getPixel(bitmap.width / 2, bitmap.height / 2))
                assertEquals(bitmap.width.toUInt(), photo.width)
                assertEquals(bitmap.height.toUInt(), photo.height)
                val preview = checkPreview(photo.thumbnail)
                val center = (preview.height.toInt() / 2 * preview.width.toInt() + preview.width.toInt() / 2) * 4
                assertRgb(
                    expected,
                    Color.rgb(
                        preview.pixels[center].toInt() and 255,
                        preview.pixels[center + 1].toInt() and 255,
                        preview.pixels[center + 2].toInt() and 255
                    )
                )
            } finally {
                bitmap.recycle()
            }
        }
    }

    @Test
    fun profileAndChatApplyTheirDistinctCoreDecodeTargets() {
        val source = encoded(2600, 1300, Color.RED, Bitmap.CompressFormat.PNG)
        val profile = preparePhoto(source, PhotoPurpose.PROFILE)
        val chat = preparePhoto(source, PhotoPurpose.CHAT)
        assertEquals(2048u, profile.width)
        assertEquals(1024u, profile.height)
        assertEquals(2560u, chat.width)
        assertEquals(1280u, chat.height)
        assertFalse(profile.full.isEmpty())
        assertFalse(chat.thumbnail.isEmpty())
    }

    @Test
    fun exifRotationIsAppliedBeforeCoreEncodingIncludingDownscaling() {
        for (width in listOf(80, 2600)) {
            val bitmap = createBitmap(width, width / 2)
            val source = try {
                val row = IntArray(width) { x -> if (x < width / 2) Color.RED else Color.BLUE }
                repeat(bitmap.height) { y -> bitmap.setPixels(row, 0, width, 0, y, width, 1) }
                ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, 95, it) }.toByteArray()
            } finally {
                bitmap.recycle()
            }
            val rotated = rotatedExif(source)
            val photo = preparePhoto(rotated, PhotoPurpose.CHAT)
            assertEquals(photo.height, photo.width * 2u)
            val decoded = requireNotNull(BitmapFactory.decodeByteArray(photo.full, 0, photo.full.size))
            try {
                assertRgb(Color.RED, decoded.getPixel(decoded.width / 2, decoded.height / 4))
                assertRgb(Color.BLUE, decoded.getPixel(decoded.width / 2, decoded.height * 3 / 4))
            } finally {
                decoded.recycle()
            }
        }
    }

    @Test
    fun unsupportedAndOversizedSourcesKeepTheirUserFacingErrors() {
        assertEquals(
            R.string.profile_photo_unsupported,
            assertThrows(PhotoProblem::class.java) { preparePhoto("GIF89a".toByteArray()) }.text
        )
        assertEquals(
            R.string.profile_photo_too_large,
            assertThrows(PhotoProblem::class.java) { preparePhoto(ByteArray(64 * 1024 * 1024 + 1)) }.text
        )
    }

    private fun encoded(width: Int, height: Int, color: Int, format: Bitmap.CompressFormat): ByteArray {
        val bitmap = createBitmap(width, height).apply { eraseColor(color) }
        return try {
            ByteArrayOutputStream().also { bitmap.compress(format, 100, it) }.toByteArray()
        } finally {
            bitmap.recycle()
        }
    }

    // A JPEG APP1 segment with a little-endian TIFF IFD containing only Orientation=6 (90 degrees CW).
    private fun rotatedExif(jpeg: ByteArray): ByteArray {
        val app1 = ByteBuffer.allocate(36)
            .putShort(0xffe1.toShort())
            .putShort(34)
            .put("Exif\u0000\u0000".toByteArray())
            .order(ByteOrder.LITTLE_ENDIAN)
            .putShort(0x4949)
            .putShort(42)
            .putInt(8)
            .putShort(1)
            .putShort(0x0112)
            .putShort(3)
            .putInt(1)
            .putShort(6)
            .putShort(0)
            .putInt(0)
            .array()
        return jpeg.copyOfRange(0, 2) + app1 + jpeg.copyOfRange(2, jpeg.size)
    }

    private fun assertRgb(expected: Int, actual: Int) {
        // JPEG is lossy; color values are the codec contract here, not a UI readiness signal.
        assertEquals(Color.red(expected).toDouble(), Color.red(actual).toDouble(), 4.0)
        assertEquals(Color.green(expected).toDouble(), Color.green(actual).toDouble(), 4.0)
        assertEquals(Color.blue(expected).toDouble(), Color.blue(actual).toDouble(), 4.0)
    }
}
