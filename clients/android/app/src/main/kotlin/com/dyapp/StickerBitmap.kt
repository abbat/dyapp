package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import uniffi.dyapp.ChatStickerFrame

private const val RGBA_CHANNELS = 4
private const val RGBA_ALPHA = 3

/** Android premultiplies these straight-alpha colors; raw RGBA copying would corrupt transparency. */
internal fun stickerBitmap(frame: ChatStickerFrame): androidx.compose.ui.graphics.ImageBitmap {
    val pixels = frame.pixels
    val colors = IntArray(pixels.size / RGBA_CHANNELS) { index ->
        val offset = index * RGBA_CHANNELS
        Color.argb(
            pixels[offset + RGBA_ALPHA].toUByte().toInt(),
            pixels[offset].toUByte().toInt(),
            pixels[offset + 1].toUByte().toInt(),
            pixels[offset + 2].toUByte().toInt()
        )
    }
    return Bitmap.createBitmap(colors, frame.width.toInt(), frame.height.toInt(), Bitmap.Config.ARGB_8888)
        .asImageBitmap()
}
