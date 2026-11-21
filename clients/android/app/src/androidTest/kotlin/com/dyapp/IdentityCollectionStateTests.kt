package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

private const val COLLECTION_TRANSITION_MS = 30_000L

class IdentityCollectionStateTests {
    @Test
    fun failedRefreshRetainsTheSnapshotAndAConfirmedRetryReplacesIt() = runBlocking {
        val state = IdentityCollectionState()
        val original = Shown(Opened.Picker)
        state.refresh { original }
        assertSame(original, state.snapshot)
        assertFalse(state.failed)
        state.refresh { error("Controlled native read failure") }
        assertTrue(state.failed)
        assertSame(original, state.snapshot)
        state.refresh { Shown(null) }
        assertTrue(state.failed)
        assertSame(original, state.snapshot)
        val replacement = Shown(Opened.Setup)
        state.refresh { replacement }
        assertSame(replacement, state.snapshot)
        assertFalse(state.failed)
        assertFalse(state.busy)
    }

    @Test
    fun retiredCompletionCannotPublishAndBusyReadIsAdmittedOnlyOnce() = runBlocking {
        val old = IdentityCollectionState()
        val entered = CompletableDeferred<Unit>()
        val response = CompletableFuture<Shown>()
        val work = launch {
            old.refresh {
                entered.complete(Unit)
                response.get(COLLECTION_TRANSITION_MS, TimeUnit.MILLISECONDS)
            }
        }
        withTimeout(COLLECTION_TRANSITION_MS) { entered.await() }
        var additionalReads = 0
        old.refresh {
            additionalReads++
            Shown(Opened.Picker)
        }
        assertEquals(0, additionalReads)
        val current = IdentityCollectionState()
        val known = Shown(Opened.Setup)
        current.refresh { known }
        old.retire()
        old.choose(Opened.Picker)
        response.complete(Shown(Opened.Picker))
        withTimeout(COLLECTION_TRANSITION_MS) { work.join() }
        assertNull(old.snapshot)
        assertNull(old.removing)
        assertSame(known, current.snapshot)
    }
}
