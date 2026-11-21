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
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.atomic.AtomicInteger

class StickerDeletePresentationTests {
    @Test
    fun cancelAndReopenRequireTheNewConfirmationAndQueuedCommitCannotRepeat() = runBlocking {
        val state = StickerDeletePresentation()
        val deletes = AtomicInteger()
        val delete = {
            deletes.incrementAndGet()
            ChatSubmission.Queued(byteArrayOf(1), "offline")
        }
        state.ask()
        val old = checkNotNull(state.pending)
        state.cancel(old)
        assertEquals(0, deletes.get())
        state.ask()
        val current = checkNotNull(state.pending)
        assertNotSame(old, current)
        state.cancel(old)
        state.confirm(old, delete)
        assertEquals(0, deletes.get())
        state.confirm(current, delete)
        assertTrue(state.submitted is ChatSubmission.Queued)
        assertNull(state.pending)
        state.confirm(current, delete)
        state.ask()
        assertEquals(1, deletes.get())
        assertNull(state.pending)
    }

    @Test
    fun failedPrecommitCanRetryOnlyThroughItsStillCurrentConfirmation() = runBlocking {
        val state = StickerDeletePresentation()
        state.ask()
        val token = checkNotNull(state.pending)
        val deletes = AtomicInteger()
        state.confirm(token) {
            deletes.incrementAndGet()
            error("controlled precommit refusal")
        }
        assertTrue(state.failed)
        assertFalse(state.busy)
        state.cancel(token)
        state.confirm(token) {
            deletes.incrementAndGet()
            error("retired confirmation must not invoke native")
        }
        assertEquals(1, deletes.get())
        assertNull(state.pending)
        assertNull(state.submitted)
    }

    @Test
    fun duplicateAndRetiredConfirmationCannotPublishNoncancellableNativeCompletion() = runBlocking {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val deletes = AtomicInteger()
        val state = StickerDeletePresentation()
        state.ask()
        val token = checkNotNull(state.pending)
        val job = launch(start = CoroutineStart.UNDISPATCHED) {
            state.confirm(token) {
                deletes.incrementAndGet()
                entered.complete(Unit)
                runBlocking { withContext(NonCancellable) { release.await() } }
                ChatSubmission.Queued(byteArrayOf(1), "offline")
            }
        }
        try {
            withTimeout(10_000) { entered.await() }
            state.confirm(token) {
                deletes.incrementAndGet()
                error("duplicate admission")
            }
            state.retire()
        } finally { release.complete(Unit) }
        withTimeout(10_000) { job.join() }
        assertEquals(1, deletes.get())
        assertNull(state.submitted)
        assertNull(state.pending)
        assertFalse(state.busy)
    }
}
