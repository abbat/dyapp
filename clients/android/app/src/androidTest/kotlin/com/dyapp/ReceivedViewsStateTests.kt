package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ReceivedProfileView

class ReceivedViewsStateTests {
    private val rows = listOf(ReceivedProfileView("original-peer", "2023-11-14"))

    @Test
    fun aFailedRefreshRetainsKnownRowsAndAnExplicitReadCanRecover() = runBlocking {
        val state = ReceivedViewsState()
        state.load { rows }
        state.load { error("read failed") }
        assertEquals(rows, state.rows)
        assertTrue(state.failed)
        assertFalse(state.busy)
        assertTrue(state.admits(rows.single().peerId))
        state.load { emptyList() }
        assertEquals(emptyList<ReceivedProfileView>(), state.rows)
        assertFalse(state.failed)
        assertFalse(state.admits(rows.single().peerId))
    }

    @Test
    fun aRetiredOpeningCannotPublishALateReadOrActOnItsReplacement() = runBlocking {
        val old = ReceivedViewsState()
        val fresh = ReceivedViewsState()
        val entered = CompletableDeferred<Unit>()
        val released = CompletableDeferred<List<ReceivedProfileView>>()
        var reads = 0
        val work = async {
            old.load {
                reads += 1
                entered.complete(Unit)
                released.await()
            }
        }
        withTimeout(CHECK_WAIT_MS) { entered.await() }
        old.retire()
        fresh.load { rows }
        released.complete(rows)
        withTimeout(CHECK_WAIT_MS) { work.await() }
        old.load {
            reads += 1
            rows
        }
        assertEquals(1, reads)
        assertNull(old.rows)
        assertFalse(old.admits(rows.single().peerId))
        assertTrue(fresh.admits(rows.single().peerId))
    }

    @Test
    fun pendingReadsDisableActionsAndRejectDuplicateRefreshes() = runBlocking {
        val state = ReceivedViewsState()
        state.load { rows }
        val entered = CompletableDeferred<Unit>()
        val released = CompletableDeferred<Unit>()
        val work = async {
            state.load {
                entered.complete(Unit)
                released.await()
                rows
            }
        }
        withTimeout(CHECK_WAIT_MS) { entered.await() }
        assertTrue(state.busy)
        assertFalse(state.admits(rows.single().peerId))
        var duplicateReads = 0
        state.load {
            duplicateReads += 1
            rows
        }
        assertEquals(0, duplicateReads)
        released.complete(Unit)
        withTimeout(CHECK_WAIT_MS) { work.await() }
        assertTrue(state.admits(rows.single().peerId))
    }
}
