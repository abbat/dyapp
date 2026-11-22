package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
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
import java.util.concurrent.CountDownLatch

class LinkApprovalLifetimeTests {
    @Test
    fun admittedSendSurvivesWaiterCancellationAndClosesOnlyAfterItsCompletion() = runBlocking {
        val fixture = LinkApprovalFixture()
        val successor = LinkApprovalFixture()
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        fixture.sending.set {
            entered.complete(Unit)
            release.await()
        }
        try {
            val opening = fixture.prepare()
            val completion = checkNotNull(fixture.session.confirm(opening))
            withTimeout(5_000) { entered.await() }
            assertNull(fixture.session.confirm(opening))
            cancelWaiter(completion)
            val retired = fixture.session.retire()
            assertRetiredAdmission(fixture, retired)
            successor.prepare()
            release.countDown()
            fixture.await(completion)
            withTimeout(5_000) { retired.await().getOrThrow() }
            assertCompletedOriginal(fixture, successor, opening)
        } finally {
            release.countDown()
            fixture.close()
            successor.close()
        }
        assertEquals(1, fixture.closed.get())
    }

    private fun assertRetiredAdmission(
        fixture: LinkApprovalFixture,
        retired: Deferred<Result<Unit>>
    ) {
        assertFalse(retired.isCompleted)
        assertEquals(0, fixture.closed.get())
        assertNull(fixture.session.prepare("another code"))
    }

    private fun assertCompletedOriginal(
        fixture: LinkApprovalFixture,
        successor: LinkApprovalFixture,
        opening: LinkApprovalOpening
    ) {
        val delivery = checkNotNull(fixture.session.state.value.delivery)
        assertSame(opening, delivery.opening)
        assertTrue(delivery.result.isSuccess)
        assertEquals(1, fixture.sent.get())
        assertNull(successor.session.state.value.delivery)
        assertEquals(0, successor.sent.get())
        assertEquals(0, successor.closed.get())
    }

    private suspend fun cancelWaiter(completion: Deferred<Unit>) = coroutineScope {
        val entered = CompletableDeferred<Unit>()
        val waiter = launch {
            entered.complete(Unit)
            completion.await()
        }
        entered.await()
        waiter.cancelAndJoin()
    }
}
