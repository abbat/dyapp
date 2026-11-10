package com.dyapp

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.ImageDecoder
import android.net.Uri
import uniffi.dyapp.PhotoDecodePlan
import uniffi.dyapp.PhotoPreparationException
import uniffi.dyapp.PhotoPurpose
import uniffi.dyapp.PreparedPhotoData
import uniffi.dyapp.photoDecodePlan
import uniffi.dyapp.photoPickedByteLimit
import uniffi.dyapp.preparePhotoPixels
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.nio.ByteBuffer

private const val READ_CHUNK = 64 * 1024
private const val RGBA_CHANNELS = 4

/** A picked file the editor will not use; [text] names why on the status line. */
internal class PhotoProblem(val text: Int) : RuntimeException()

/** Metadata-free original, preview and actual dimensions supplied by the common core. */
internal typealias PreparedPhoto = PreparedPhotoData

/** Reads only within the core's source budget. Call it off the UI thread. */
internal fun readPicked(context: Context, uri: Uri): ByteArray =
    (context.contentResolver.openInputStream(uri) ?: throw PhotoProblem(R.string.profile_photo_unsupported))
        .use(::readBounded)

private fun readBounded(input: InputStream): ByteArray {
    val limit = photoPickedByteLimit()
    val out = ByteArrayOutputStream()
    val chunk = ByteArray(READ_CHUNK)
    while (true) {
        val read = input.read(chunk)
        if (read < 0) return out.toByteArray()
        if (out.size().toULong() + read.toULong() > limit) throw PhotoProblem(R.string.profile_photo_too_large)
        out.write(chunk, 0, read)
    }
}

/** Android decodes upright pixels; the core owns all preparation policy. Call off the UI thread. */
internal fun preparePhoto(bytes: ByteArray, purpose: PhotoPurpose = PhotoPurpose.PROFILE): PreparedPhoto {
    val bitmap = decodePhoto(bytes, purpose)
    try {
        return preparePhotoPixels(straightRgba(bitmap), bitmap.width.toUInt(), bitmap.height.toUInt(), purpose)
    } catch (error: PhotoPreparationException) {
        throw photoProblem(error)
    } finally {
        bitmap.recycle()
    }
}

/** Applies the core's header plan before allocation, also for displaying fetched originals. */
internal fun decodePhoto(bytes: ByteArray, purpose: PhotoPurpose): Bitmap = try {
    if (bytes.size.toULong() > photoPickedByteLimit()) throw PhotoProblem(R.string.profile_photo_too_large)
    var plan: PhotoDecodePlan? = null
    val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
        val target = photoDecodePlan(
            info.mimeType,
            info.size.width.toUInt(),
            info.size.height.toUInt(),
            bytes.size.toULong(),
            purpose
        )
        plan = target
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        decoder.setTargetSize(target.width.toInt(), target.height.toInt())
    }
    if (bitmap.width.toUInt() != plan?.width || bitmap.height.toUInt() != plan?.height) {
        bitmap.recycle()
        throw PhotoProblem(R.string.profile_photo_unsupported)
    }
    bitmap
} catch (error: IOException) {
    throw PhotoProblem(R.string.profile_photo_unsupported).apply { initCause(error) }
} catch (error: PhotoPreparationException) {
    throw photoProblem(error)
}

private fun photoProblem(error: PhotoPreparationException): PhotoProblem = PhotoProblem(
    when (error) {
        is PhotoPreparationException.TooLarge -> R.string.profile_photo_too_large
        is PhotoPreparationException.Unsupported -> R.string.profile_photo_unsupported
    }
).apply { initCause(error) }

// getPixels returns straight sRGB ARGB; copyPixelsToBuffer retains native packing and premultiplication.
private fun straightRgba(bitmap: Bitmap): ByteArray {
    val pixels = ByteArray(bitmap.width * bitmap.height * RGBA_CHANNELS)
    val row = IntArray(bitmap.width)
    var at = 0
    repeat(bitmap.height) { y ->
        bitmap.getPixels(row, 0, bitmap.width, 0, y, bitmap.width, 1)
        row.forEach { color ->
            pixels[at++] = Color.red(color).toByte()
            pixels[at++] = Color.green(color).toByte()
            pixels[at++] = Color.blue(color).toByte()
            pixels[at++] = Color.alpha(color).toByte()
        }
    }
    return pixels
}
