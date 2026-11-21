package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.StickerPackSaveAttempt
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus
import java.util.concurrent.atomic.AtomicInteger

class StickerPackPresentationTests {
    private fun waiting(token: Int) = StickerPackSaveStatus(
        state = StickerPackSaveState.WAITING,
        generation = ByteArray(16) { token.toByte() },
        failure = null,
        retryAtMs = null,
        controls = listOf(StickerPackSaveControl.STOP_WAITING)
    )

    @Test
    fun readbackFailurePreservesKnownIntentAndCannotRepeatAnUncertainWrite() = runBlocking {
        val first = waiting(1)
        val next = waiting(2)
        var unreadable = false
        val writes = AtomicInteger()
        val calls = StickerPackCalls(
            read = { if (unreadable) error("controlled read failure") else next },
            change = { _, _ ->
                writes.incrementAndGet()
                unreadable = true
                StickerPackSaveAttempt.Unavailable("controlled post-commit read failure")
            }
        )
        val state = StickerPackPresentation()
        state.refresh(StickerPackCalls({ first }, calls.change))
        state.change(calls, first, StickerPackSaveControl.STOP_WAITING)
        assertSame(first, state.status)
        assertTrue(state.readFailed && state.actionFailed)
        state.change(calls, first, StickerPackSaveControl.STOP_WAITING)
        assertEquals(1, writes.get())
        unreadable = false
        state.refresh(calls)
        assertSame(next, state.status)
        assertFalse(state.allows(first, StickerPackSaveControl.STOP_WAITING))
        assertTrue(state.allows(next, StickerPackSaveControl.STOP_WAITING))
    }

    @Test
    fun oldControlsCannotActAfterAStatusRefreshEvenWhenTheTokenIsUnchanged() = runBlocking {
        val first = waiting(1)
        val next = first.copy()
        val writes = AtomicInteger()
        var current = first
        val calls = StickerPackCalls({ current }, { _, _ ->
            writes.incrementAndGet()
            StickerPackSaveAttempt.Ready(next)
        })
        val state = StickerPackPresentation()
        state.refresh(calls)
        current = next
        state.refresh(calls)
        state.change(calls, first, StickerPackSaveControl.STOP_WAITING)
        assertEquals(0, writes.get())
        state.change(calls, next, StickerPackSaveControl.STOP_WAITING)
        assertEquals(1, writes.get())
    }

    @Test
    fun duplicateAdmissionAndLateCompletionCannotSurviveRetirement() = runBlocking {
        val first = waiting(1)
        val started = CompletableDeferred<Unit>()
        val finish = CompletableDeferred<Unit>()
        val writes = AtomicInteger()
        val calls = StickerPackCalls({ first }, { _, _ ->
            writes.incrementAndGet()
            started.complete(Unit)
            withContext(NonCancellable) { finish.await() }
            StickerPackSaveAttempt.Ready(waiting(2))
        })
        val state = StickerPackPresentation()
        state.refresh(calls)
        val pending = launch(start = CoroutineStart.UNDISPATCHED) {
            state.change(calls, first, StickerPackSaveControl.STOP_WAITING)
        }
        try {
            withTimeout(10_000) { started.await() }
            state.change(calls, first, StickerPackSaveControl.STOP_WAITING)
            assertEquals(1, writes.get())
            state.retire()
        } finally {
            finish.complete(Unit)
            withTimeout(10_000) { pending.join() }
        }
        assertSame(first, state.status)
        assertFalse(state.allows(first, StickerPackSaveControl.STOP_WAITING))
    }
}
