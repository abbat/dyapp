package com.dyapp

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
import android.net.Uri
import uniffi.dyapp.makePreview
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.nio.ByteBuffer

private const val MAX_PICKED_BYTES = 64 * 1024 * 1024
private const val MAX_PIXELS = 64_000_000L
private const val MAX_FULL_BYTES = 1024 * 1024
private const val MAX_THUMBNAIL_BYTES = 24 * 1024
private const val THUMBNAIL_SIDE = 320
private const val READ_CHUNK = 64 * 1024
private val FULL_SIDES = intArrayOf(2048, 1600, 1280)
private val FULL_QUALITIES = intArrayOf(85, 70, 55)
private val PICKED_TYPES = setOf("image/jpeg", "image/png")

/** A picked file the editor will not use; [text] names why on the status line. */
internal class PhotoProblem(val text: Int) : RuntimeException()

/** A profile photo ready for `add_profile_photo`: bare JPEGs, no EXIF, GPS or colour profile. */
internal class PreparedPhoto(val full: ByteArray, val thumbnail: ByteArray)

/** Reads the picked file, refusing it past 64 MiB without reading further. Call it off the UI thread. */
internal fun readPicked(context: Context, uri: Uri): ByteArray =
    (context.contentResolver.openInputStream(uri) ?: throw PhotoProblem(R.string.profile_photo_unsupported))
        .use(::readBounded)

private fun readBounded(input: InputStream): ByteArray {
    val out = ByteArrayOutputStream()
    val chunk = ByteArray(READ_CHUNK)
    while (true) {
        val read = input.read(chunk)
        if (read < 0) return out.toByteArray()
        out.write(chunk, 0, read)
        if (out.size() > MAX_PICKED_BYTES) throw PhotoProblem(R.string.profile_photo_too_large)
    }
}

/**
 * Decodes a JPEG or PNG within 64 megapixels upright, flattens it onto white and writes the full
 * image at 2048, 1600 or 1280 px within 1 MiB and its thumbnail; see docs/design/profile-photos.md.
 * Call it off the UI thread.
 */
internal fun preparePhoto(bytes: ByteArray): PreparedPhoto {
    if (bytes.size > MAX_PICKED_BYTES) throw PhotoProblem(R.string.profile_photo_too_large)
    val decoded = decode(bytes)
    val flat = Bitmap.createBitmap(decoded.width, decoded.height, Bitmap.Config.ARGB_8888)
    Canvas(flat).apply {
        drawColor(Color.WHITE)
        drawBitmap(decoded, 0f, 0f, null)
    }
    decoded.recycle()
    try {
        val full = FULL_SIDES.asSequence().flatMap { side -> FULL_QUALITIES.asSequence().map { side to it } }
            .map { (side, quality) -> jpeg(flat, side, quality) }
            .firstOrNull { it.size <= MAX_FULL_BYTES }
            ?: throw PhotoProblem(R.string.profile_photo_too_large)
        return PreparedPhoto(full, thumbnail(flat))
    } finally {
        flat.recycle()
    }
}

// Upright, at most 2048 px on the long side; the header is checked before any pixel is decoded.
private fun decode(bytes: ByteArray): Bitmap = try {
    ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
        checkHeader(info)
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        fitted(info.size.width, info.size.height, FULL_SIDES[0])?.let { (w, h) -> decoder.setTargetSize(w, h) }
    }
} catch (error: IOException) {
    throw PhotoProblem(R.string.profile_photo_unsupported).apply { initCause(error) }
}

private fun checkHeader(info: ImageDecoder.ImageInfo) {
    if (info.mimeType !in PICKED_TYPES) throw PhotoProblem(R.string.profile_photo_unsupported)
    if (info.size.width.toLong() * info.size.height > MAX_PIXELS) throw PhotoProblem(R.string.profile_photo_too_large)
}

// The size within [side] keeping the aspect, or null when it already fits.
internal fun fitted(width: Int, height: Int, side: Int): Pair<Int, Int>? {
    val longest = maxOf(width, height)
    if (longest <= side) return null
    return maxOf(1, width * side / longest) to maxOf(1, height * side / longest)
}

private fun scaled(bitmap: Bitmap, side: Int): Bitmap =
    fitted(bitmap.width, bitmap.height, side)?.let { (w, h) -> Bitmap.createScaledBitmap(bitmap, w, h, true) }
        ?: bitmap

private fun jpeg(bitmap: Bitmap, side: Int, quality: Int): ByteArray {
    val image = scaled(bitmap, side)
    val out = ByteArrayOutputStream()
    check(image.compress(Bitmap.CompressFormat.JPEG, quality, out))
    if (image !== bitmap) image.recycle()
    return out.toByteArray()
}

// The core writes the thumbnail, the same encoder chat previews use.
private fun thumbnail(bitmap: Bitmap): ByteArray {
    val image = scaled(bitmap, THUMBNAIL_SIDE)
    val rgba = ByteBuffer.allocate(image.byteCount)
    image.copyPixelsToBuffer(rgba)
    val (width, height) = image.width.toUInt() to image.height.toUInt()
    if (image !== bitmap) image.recycle()
    return makePreview(rgba.array(), width, height, MAX_THUMBNAIL_BYTES.toUInt())
}
