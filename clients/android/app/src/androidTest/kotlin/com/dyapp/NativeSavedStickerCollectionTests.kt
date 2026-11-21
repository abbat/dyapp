package com.dyapp

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.dyapp.CoreException

class NativeSavedStickerCollectionTests {
    @Test
    fun freshIdentityCanReadLocalPacksAndRejectedInputPreservesTheCollection() {
        NativeSlotFixture().use { fixture ->
            val opened = fixture.slots.create()
            val calls = savedStickerCalls(opened)
            val before = runBlocking { calls.page(null) }
            assertEquals(emptyList<uniffi.dyapp.SavedStickerPackEntry>(), before.entries)
            assertThrows(CoreException::class.java) { runBlocking { calls.save(byteArrayOf(1, 2, 3)) } }
            assertFalse(runBlocking { calls.remove(ByteArray(32) { 1 }) })
            val after = runBlocking { calls.page(null) }
            assertEquals(before, after)
        }
    }
}
