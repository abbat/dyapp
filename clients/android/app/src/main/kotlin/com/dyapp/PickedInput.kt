package com.dyapp

import java.io.ByteArrayOutputStream
import java.io.InputStream

private const val PICKED_READ_CHUNK = 64 * 1024

/** OS transport only: retain at most the core budget and consume at most one extra byte to detect overflow. */
internal fun readPickedBytes(input: InputStream, limit: ULong): ByteArray {
    val out = ByteArrayOutputStream()
    val chunk = ByteArray(PICKED_READ_CHUNK)
    while (true) {
        val remaining = limit - out.size().toULong()
        val wanted = remaining.coerceAtMost((chunk.size - 1).toULong()).toInt() + 1
        val read = input.read(chunk, 0, wanted)
        if (read < 0) return out.toByteArray()
        if (read.toULong() > remaining) throw PickedInputTooLarge()
        out.write(chunk, 0, read)
    }
}
