package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private class ForwardCompletionGate(fixture: ForwardStateFixture) {
    val entered = CompletableDeferred<Unit>()
    val response = CompletableFuture<ChatSubmission>()
    val sends = AtomicInteger()
    val calls = ChatForwardCalls(fixture.calls.read) { _, _ ->
        sends.incrementAndGet()
        entered.complete(Unit)
        response.get(60, TimeUnit.SECONDS)
    }
}

class ChatForwardCompletionTests {
    @Test
    fun lateCommittedNativeResultCannotPublishOrNavigateAfterOwnerRetirement() = runBlocking {
        completeLate(cancelled = false)
    }

    @Test
    fun cancelledCallerDropsNativeCompletionInsteadOfReportingItAsFailure() = runBlocking {
        completeLate(cancelled = true)
    }

    private suspend fun completeLate(cancelled: Boolean) = coroutineScope {
        val f = ForwardStateFixture()
        val shown = f.ready()
        val gate = ForwardCompletionGate(f)
        val pending = launch(start = CoroutineStart.UNDISPATCHED) { f.state.send(shown, gate.calls) }
        try {
            withTimeout(60_000) { gate.entered.await() }
            assertTrue(f.state.busy)
            f.state.send(shown, gate.calls)
            f.state.cancel(shown)
            assertSame(shown, f.state.opening)
            assertEquals(1, gate.sends.get())
            f.state.retire()
            val replacement = ForwardStateFixture()
            val current = replacement.ready()
            if (cancelled) pending.cancel()
            gate.response.complete(ChatSubmission.Queued(ByteArray(16), "committed late"))
            withTimeout(60_000) { pending.join() }
            requireRetired(f, replacement, current)
        } finally {
            gate.response.complete(ChatSubmission.Accepted(ByteArray(16)))
            pending.cancelAndJoin()
        }
    }

    private fun requireRetired(
        previous: ForwardStateFixture,
        replacement: ForwardStateFixture,
        shown: ForwardSelection,
    ) {
        assertNull(previous.state.opening)
        assertNull(previous.state.submitted)
        assertNull(previous.state.failure)
        assertFalse(previous.state.busy)
        assertSame(shown, replacement.state.opening)
        assertEquals(0, replacement.sends)
    }
}
