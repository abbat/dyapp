package com.dyapp

import java.nio.ByteBuffer
import java.util.zip.CRC32

private const val PNG_SIGNATURE_BYTES = 8
private const val IHDR_BYTES = 13
internal const val OVERSIZED_PNG_WIDTH = 8_001
internal const val OVERSIZED_PNG_HEIGHT = 8_000

/** Change only the parsed IHDR dimensions/CRC; no large pixel buffer is created. */
internal fun oversizedPng(source: ByteArray): ByteArray {
    val frame = ByteBuffer.wrap(source.copyOf())
    frame.position(PNG_SIGNATURE_BYTES)
    val length = frame.int
    check(length == IHDR_BYTES)
    val type = ByteArray(Int.SIZE_BYTES).also { frame.get(it) }
    check(type.contentEquals("IHDR".toByteArray(Charsets.US_ASCII)))
    val payload = frame.position()
    frame.putInt(OVERSIZED_PNG_WIDTH)
    frame.putInt(OVERSIZED_PNG_HEIGHT)
    val crc = CRC32().apply {
        update(type)
        update(frame.array(), payload, length)
    }
    frame.putInt(payload + length, crc.value.toInt())
    return frame.array()
}
