package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.dyapp.PhotoPurpose
import java.io.ByteArrayOutputStream

/** Direct original decoding, without the outgoing preparation that converts PNG to JPEG. */
class NativeOriginalDecodeTests {
    @Test
    fun pngAndJpegOriginalsUseTheActualPlatformDecoderAndNativeHeaderPlan() {
        for (format in listOf(Bitmap.CompressFormat.PNG, Bitmap.CompressFormat.JPEG)) {
            val bytes = encoded(format)
            val decoded = decodePhoto(bytes, PhotoPurpose.CHAT)
            try {
                assertEquals(20, decoded.width)
                assertEquals(40, decoded.height)
            } finally {
                decoded.recycle()
            }
        }
    }

    @Test
    fun truncatedOriginalsFailWithTheExistingSafeError() {
        for (format in listOf(Bitmap.CompressFormat.PNG, Bitmap.CompressFormat.JPEG)) {
            // Keep the format signature but omit all actual image data.
            val truncated = encoded(format).copyOf(if (format == Bitmap.CompressFormat.PNG) 8 else 2)
            val failure = assertThrows(PhotoProblem::class.java) {
                decodePhoto(truncated, PhotoPurpose.CHAT)
            }
            assertEquals(R.string.profile_photo_unsupported, failure.text)
        }
    }

    private fun encoded(format: Bitmap.CompressFormat): ByteArray {
        val bitmap = createBitmap(20, 40).apply { eraseColor(Color.RED) }
        return try {
            ByteArrayOutputStream().also { check(bitmap.compress(format, 100, it)) }.toByteArray()
        } finally {
            bitmap.recycle()
        }
    }
}
