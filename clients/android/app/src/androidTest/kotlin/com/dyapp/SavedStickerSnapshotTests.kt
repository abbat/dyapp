package com.dyapp

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.SavedStickerPackState
import java.util.concurrent.atomic.AtomicInteger

class SavedStickerSnapshotTests {
    @Test
    fun anEqualFreshPageRetiresThePreviousPageCallback() = runBlocking {
        val entry = SavedStickerPackEntry(ByteArray(32), 0uL, SavedStickerPackState.Unreadable("fixture"))
        val first = SavedStickerPackPage(listOf(entry), ByteArray(32) { 1 }, 0uL, 0uL)
        var current = first
        val reads = AtomicInteger()
        val calls = SavedStickerCalls({
            reads.incrementAndGet()
            current
        }, { error("unexpected import") }, { error("unexpected removal") })
        val state = SavedStickerPresentation()
        state.refresh(calls)
        current = first.copy()
        state.refresh(calls)
        assertSame(current, state.page)
        state.refresh(calls, first.next, first)
        assertEquals(2, reads.get())
    }
}
