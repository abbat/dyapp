package com.dyapp

import kotlinx.coroutines.Deferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.SearchRecoveryException
import uniffi.dyapp.SearchRecoverySaved
import java.util.concurrent.atomic.AtomicInteger

class SearchRecoveryFailureTests {
    private val queued = SearchRecoverySaved(2uL, true, true)

    @Test
    fun refusedConfirmationNeedsFreshExplicitReviewAndCannotRetryOrResign() = runBlocking {
        val prepared = AtomicInteger()
        val confirmed = AtomicInteger()
        val closed = AtomicInteger()
        val session = SearchRecoverySession {
            val generation = prepared.incrementAndGet()
            SearchRecoveryCalls({
                confirmed.incrementAndGet()
                check(generation > 1) { "Controlled changed review" }
                queued
            }, { error("Unconfirmed record must not be delivered") }, { closed.incrementAndGet() })
        }
        try {
            await(session.prepare(null))
            val old = checkNotNull(session.state.value.opening)
            await(session.confirm(old))
            assertRefused(session, old)
            await(session.prepare(old))
            assertNull(session.confirm(old))
            await(session.confirm(checkNotNull(session.state.value.opening)))
            assertEquals(queued, session.state.value.saved)
            assertEquals(2, confirmed.get())
        } finally {
            finish(session)
        }
        assertEquals(2, closed.get())
    }

    @Test
    fun failedDeliveryKeepsConfirmedReceiptAndRetryNeverSignsAgain() = runBlocking {
        val confirmed = AtomicInteger()
        val retried = AtomicInteger()
        val acknowledged = SearchRecoverySaved(2uL, true, false)
        val session = SearchRecoverySession {
            SearchRecoveryCalls({
                confirmed.incrementAndGet()
                queued
            }, {
                if (retried.incrementAndGet() == 1) throw SearchRecoveryException.Failed("Controlled transport failure")
                acknowledged
            }, {})
        }
        try {
            await(session.prepare(null))
            val opening = checkNotNull(session.state.value.opening)
            await(session.confirm(opening))
            await(session.retry(opening))
            assertFailedReceipt(session, opening)
            await(session.retry(opening))
            assertEquals(acknowledged, session.state.value.saved)
            assertNull(session.state.value.failure)
            assertNull(session.retry(opening))
            assertEquals(1, confirmed.get())
            assertEquals(2, retried.get())
        } finally {
            finish(session)
        }
    }

    @Test
    fun failedRefreshCannotRetainAnActionForAReleasedHandle() = runBlocking {
        val prepared = AtomicInteger()
        val closed = AtomicInteger()
        val session = SearchRecoverySession {
            check(prepared.incrementAndGet() != 2) { "Controlled refresh failure" }
            SearchRecoveryCalls({ queued }, { error("Unexpected retry") }, { closed.incrementAndGet() })
        }
        try {
            await(session.prepare(null))
            val old = checkNotNull(session.state.value.opening)
            await(session.prepare(old))
            assertNull(session.state.value.opening)
            assertNotNull(session.state.value.failure)
            assertNull(session.confirm(old))
            assertNull(session.retry(old))
            assertEquals(1, closed.get())
            await(session.prepare(null))
            await(session.confirm(checkNotNull(session.state.value.opening)))
            assertEquals(queued, session.state.value.saved)
        } finally {
            finish(session)
        }
        assertEquals(2, closed.get())
    }

    @Test
    fun typedReviewRefusalsRequireRefreshInsteadOfRepeatingDelivery() = runBlocking {
        for (failure in listOf(
            SearchRecoveryException.Closed(),
            SearchRecoveryException.Changed(),
            SearchRecoveryException.ReviewRequired(),
            SearchRecoveryException.LinkPending(),
            SearchRecoveryException.VersionExhausted(),
            SearchRecoveryException.WrongOwner()
        )) {
            checkRefusal(failure)
        }
    }

    private suspend fun checkRefusal(failure: SearchRecoveryException) {
        val session = SearchRecoverySession { SearchRecoveryCalls({ queued }, { throw failure }, {}) }
        try {
            await(session.prepare(null))
            val opening = checkNotNull(session.state.value.opening)
            await(session.confirm(opening))
            await(session.retry(opening))
            assertEquals(failure, session.state.value.failure)
            assertNull(session.retry(opening))
            assertNull(session.confirm(opening))
            await(session.prepare(opening))
            assertNull(session.state.value.saved)
            assertNull(session.confirm(opening))
        } finally {
            finish(session)
        }
    }

    private fun assertFailedReceipt(session: SearchRecoverySession, opening: SearchRecoveryOpening) {
        assertNotNull(session.state.value.failure)
        assertEquals(queued, session.state.value.saved)
        assertNull(session.confirm(opening))
    }

    private fun assertRefused(session: SearchRecoverySession, opening: SearchRecoveryOpening) {
        assertNotNull(session.state.value.failure)
        assertNull(session.state.value.saved)
        assertNull(session.confirm(opening))
        assertNull(session.retry(opening))
    }

    private suspend fun await(done: Deferred<Unit>?) = withTimeout(5_000) { checkNotNull(done).await() }

    private suspend fun finish(session: SearchRecoverySession) =
        withTimeout(5_000) { session.retire().await().getOrThrow() }
}
