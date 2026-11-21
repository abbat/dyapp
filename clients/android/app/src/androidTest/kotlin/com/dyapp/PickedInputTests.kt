package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream

class PickedInputTests {
    @Test
    fun emptyAndExactBudgetInputsAreRetainedWithoutTruncation() {
        assertArrayEquals(byteArrayOf(), readPickedBytes(ByteArrayInputStream(byteArrayOf()), 0uL))
        val bytes = byteArrayOf(1, 2, 3, 4)
        assertArrayEquals(bytes, readPickedBytes(ByteArrayInputStream(bytes), bytes.size.toULong()))
    }

    @Test
    fun oversizedSourcesConsumeOnlyTheSingleByteNeededToDetectOverflow() {
        val input = ByteArrayInputStream(ByteArray(100))
        assertThrows(PickedInputTooLarge::class.java) { readPickedBytes(input, 4uL) }
        assertEquals(95, input.available())
        val zero = ByteArrayInputStream(byteArrayOf(1, 2))
        assertThrows(PickedInputTooLarge::class.java) { readPickedBytes(zero, 0uL) }
        assertEquals(1, zero.available())
    }

    @Test
    fun fragmentedReadsPreserveEveryByte() {
        val bytes = byteArrayOf(1, 2, 3, 4)
        val input = object : ByteArrayInputStream(bytes) {
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
                super.read(buffer, offset, minOf(1, length))
        }
        assertArrayEquals(bytes, readPickedBytes(input, bytes.size.toULong()))
    }

    @Test
    fun providerFailuresRemainFailuresRatherThanPartialFiles() {
        val failure = IOException("controlled provider failure")
        val input = object : InputStream() {
            private var first = true
            override fun read(): Int {
                if (!first) throw failure
                first = false
                return 1
            }
        }
        assertSame(failure, assertThrows(IOException::class.java) { readPickedBytes(input, 4uL) })
    }
}
