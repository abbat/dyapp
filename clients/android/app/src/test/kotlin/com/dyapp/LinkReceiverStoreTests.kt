package com.dyapp

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

class LinkReceiverStoreTests {
    private fun source() = Opened.Ready(Slot("original", File("presentation-only"), SlotState.READY), "peer")

    @Test
    fun pendingOriginalBlocksReplacementAndRetiredDispositionCannotClearAnEqualSuccessor() = runBlocking {
        val creates = AtomicInteger()
        val store = LinkReceiverStore { _, _ ->
            LinkReceiverSession {
                creates.incrementAndGet()
                error("controlled invitation refusal")
            }
        }
        val original = source()
        val opening = checkNotNull(store.begin(original, false))
        withTimeout(5_000) { opening.session.state.first { !it.busy } }
        assertSame(original, opening.source)
        assertNull(store.begin(source(), true))
        assertEquals(1, creates.get())
        opening.session.retire()
        assertSame(opening, store.state.value)
        assertTrue(store.release(opening))
        val next = checkNotNull(store.begin(source(), false))
        withTimeout(5_000) { next.session.state.first { !it.busy } }
        assertNotSame(opening, next)
        assertNotSame(original, next.source)
        assertFalse(store.release(opening))
        assertSame(next, store.state.value)
        assertEquals(2, creates.get())
        assertTrue(store.release(next))
        assertNull(store.state.value)
    }
}
