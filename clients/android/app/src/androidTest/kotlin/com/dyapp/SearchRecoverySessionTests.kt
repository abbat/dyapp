package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.SearchRecoverySaved
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger

class SearchRecoverySessionTests {
    private val saved = SearchRecoverySaved(2uL, true, true)

    @Test
    fun preparationAndRefreshNeverConfirmAndOldOpeningCannotAdmitWork() = runBlocking {
        val confirmed = AtomicInteger()
        val retried = AtomicInteger()
        val closed = AtomicInteger()
        val session = SearchRecoverySession { countedCalls(confirmed, retried, closed) }
        try {
            await(session.prepare(null))
            val old = checkNotNull(session.state.value.opening)
            assertEquals(0, confirmed.get())
            assertNull(session.retry(old))
            await(session.prepare(old))
            val current = checkNotNull(session.state.value.opening)
            assertOldOpeningRejected(session, old, closed)
            await(session.confirm(current))
            assertEquals(saved, session.state.value.saved)
            assertNull(session.confirm(current))
            await(session.retry(current))
            assertEquals(1, confirmed.get())
            assertEquals(1, retried.get())
        } finally {
            finish(session)
        }
        assertEquals(2, closed.get())
    }

    @Test
    fun retirementDuringPreparationReleasesLateHandleWithoutConfirming() = runBlocking {
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val closed = AtomicInteger()
        val session = SearchRecoverySession {
            entered.complete(Unit)
            release.await()
            SearchRecoveryCalls({ error("unexpected confirm") }, { error("unexpected retry") }) {
                closed.incrementAndGet()
            }
        }
        try {
            val prepared = checkNotNull(session.prepare(null))
            withTimeout(5_000) { entered.await() }
            val retired = session.retire()
            assertFalse(retired.isCompleted)
            assertEquals(0, closed.get())
            assertNull(session.prepare(null))
            release.countDown()
            await(prepared)
            withTimeout(5_000) { retired.await().getOrThrow() }
            assertTrue(session.state.value.retired)
            assertEquals(1, closed.get())
        } finally {
            release.countDown()
            finish(session)
        }
    }

    @Test
    fun retiringDuringAdmittedConfirmationPreservesWriteAndClosesAfterCompletion() = runBlocking {
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val closed = AtomicInteger()
        val session = blockedConfirmation(entered, release, closed)
        try {
            await(session.prepare(null))
            val opening = checkNotNull(session.state.value.opening)
            val confirmed = checkNotNull(session.confirm(opening))
            withTimeout(5_000) { entered.await() }
            assertNull(session.confirm(opening))
            val retired = session.retire()
            assertRetiredCallActive(retired, closed)
            assertNull(session.retry(opening))
            release.countDown()
            await(confirmed)
            withTimeout(5_000) { retired.await().getOrThrow() }
            assertRetiredWriteSaved(session, closed)
        } finally {
            release.countDown()
            finish(session)
        }
    }

    private fun blockedConfirmation(
        entered: CompletableDeferred<Unit>,
        release: CountDownLatch,
        closed: AtomicInteger
    ) = SearchRecoverySession {
        SearchRecoveryCalls(
            {
                entered.complete(Unit)
                release.await()
                saved
            },
            { error("unexpected retry") },
            { closed.incrementAndGet() }
        )
    }

    private fun countedCalls(confirmed: AtomicInteger, retried: AtomicInteger, closed: AtomicInteger) =
        SearchRecoveryCalls(
            {
                confirmed.incrementAndGet()
                saved
            },
            {
                retried.incrementAndGet()
                saved
            },
            { closed.incrementAndGet() }
        )

    private fun assertRetiredWriteSaved(session: SearchRecoverySession, closed: AtomicInteger) {
        assertEquals(saved, session.state.value.saved)
        assertTrue(session.state.value.retired)
        assertEquals(1, closed.get())
    }

    private fun assertOldOpeningRejected(
        session: SearchRecoverySession,
        old: SearchRecoveryOpening,
        closed: AtomicInteger
    ) {
        assertEquals(1, closed.get())
        assertNull(session.confirm(old))
        assertNull(session.prepare(old))
        assertNull(session.retry(old))
    }

    private fun assertRetiredCallActive(retired: Deferred<Result<Unit>>, closed: AtomicInteger) {
        assertFalse(retired.isCompleted)
        assertEquals(0, closed.get())
    }

    private suspend fun await(done: Deferred<Unit>?) = withTimeout(5_000) { checkNotNull(done).await() }

    private suspend fun finish(session: SearchRecoverySession) =
        withTimeout(5_000) { session.retire().await().getOrThrow() }
}
