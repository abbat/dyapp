package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.createBitmap
import uniffi.dyapp.Rgba
import java.nio.ByteBuffer

/** Copies already bounded, decoded core pixels; no platform JPEG/PNG decoder runs here. */
internal fun previewBitmap(rgba: Rgba): ImageBitmap {
    val bitmap = createBitmap(rgba.width.toInt(), rgba.height.toInt(), Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba.pixels))
    return bitmap.asImageBitmap()
}
