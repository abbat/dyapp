package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.MobileLinkCodeStatus
import java.util.concurrent.CountDownLatch

class LinkApprovalWorkerTests {
    @Test
    fun coreInputStatusAndExactOpeningControlPreparationAndConsent() = runBlocking {
        val fixture = LinkApprovalFixture()
        val successor = LinkApprovalFixture()
        try {
            assertRefusedCodes(fixture)
            fixture.status.set(MobileLinkCodeStatus.VALID)
            val opening = fixture.prepare()
            assertEquals("original pasted input", fixture.input.get())
            assertEquals(fixture.normalized, opening.code)
            assertNull(fixture.session.prepare("edited input"))
            assertNull(fixture.session.confirm(LinkApprovalOpening(opening.code)))
            successor.prepare()
            assertNull(successor.session.confirm(opening))
            fixture.await(checkNotNull(fixture.session.confirm(opening)))
            assertEquals(fixture.normalized, fixture.inspected.get())
            assertEquals(1, fixture.sent.get())
            assertTrue(checkNotNull(fixture.session.state.value.delivery).result.isSuccess)
            assertNull(fixture.session.confirm(opening))
            assertEquals(0, successor.sent.get())
        } finally {
            fixture.close()
            successor.close()
        }
        assertEquals(1, fixture.closed.get())
    }

    private suspend fun assertRefusedCodes(fixture: LinkApprovalFixture) {
        for (status in listOf(MobileLinkCodeStatus.MALFORMED, MobileLinkCodeStatus.EXPIRED)) {
            fixture.status.set(status)
            fixture.await(checkNotNull(fixture.session.prepare("original pasted input")))
            assertEquals(status, fixture.session.state.value.codeStatus)
            assertNull(fixture.session.state.value.opening)
            assertEquals(0, fixture.created.get())
        }
    }

    @Test
    fun uncertainDeliverySurvivesAReadOnlyRetryThatFindsExpiry() = runBlocking {
        val fixture = LinkApprovalFixture()
        val failure = IllegalStateException("later encrypted part refused after earlier delivery")
        fixture.sending.set { throw failure }
        try {
            val opening = fixture.prepare()
            fixture.await(checkNotNull(fixture.session.confirm(opening)))
            val delivery = checkNotNull(fixture.session.state.value.delivery)
            assertSame(opening, delivery.opening)
            assertSame(failure, delivery.result.exceptionOrNull())
            fixture.status.set(MobileLinkCodeStatus.EXPIRED)
            fixture.await(checkNotNull(fixture.session.confirm(opening)))
            assertEquals(MobileLinkCodeStatus.EXPIRED, fixture.session.state.value.codeStatus)
            assertSame(delivery, fixture.session.state.value.delivery)
            assertSame(opening, fixture.session.state.value.opening)
            assertEquals(1, fixture.sent.get())
            assertNull(fixture.session.confirm(opening))
        } finally {
            fixture.close()
        }
    }

    @Test
    fun expiryBeforeTheFirstSendDoesNotInventPartialDelivery() = runBlocking {
        val fixture = LinkApprovalFixture()
        try {
            val opening = fixture.prepare()
            fixture.status.set(MobileLinkCodeStatus.EXPIRED)
            fixture.await(checkNotNull(fixture.session.confirm(opening)))
            assertEquals(MobileLinkCodeStatus.EXPIRED, fixture.session.state.value.codeStatus)
            assertSame(opening, fixture.session.state.value.opening)
            assertNull(fixture.session.state.value.delivery)
            assertEquals(0, fixture.sent.get())
            assertNull(fixture.session.confirm(opening))
        } finally {
            fixture.close()
        }
    }

    @Test
    fun retirementDuringReadOnlyPreparationWaitsForTheActualHandle() = runBlocking {
        val fixture = LinkApprovalFixture()
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        fixture.preparing.set {
            entered.complete(Unit)
            release.await()
        }
        try {
            val completion = checkNotNull(fixture.session.prepare("original pasted input"))
            withTimeout(5_000) { entered.await() }
            assertNull(fixture.session.prepare("duplicate"))
            val retired = fixture.session.retire()
            assertFalse(retired.isCompleted)
            assertEquals(0, fixture.closed.get())
            release.countDown()
            fixture.await(completion)
            withTimeout(5_000) { retired.await().getOrThrow() }
            assertTrue(fixture.session.state.value.retired)
            assertNull(fixture.session.confirm(checkNotNull(fixture.session.state.value.opening)))
            assertEquals(0, fixture.sent.get())
        } finally {
            release.countDown()
            fixture.close()
        }
        assertEquals(1, fixture.closed.get())
    }

    @Test
    fun failedPreparationCanBeExplicitlyRetriedWithoutClaimingDelivery() = runBlocking {
        val fixture = LinkApprovalFixture()
        val failure = IllegalStateException("read-only preparation unavailable")
        fixture.preparing.set { throw failure }
        try {
            fixture.await(checkNotNull(fixture.session.prepare("original pasted input")))
            assertSame(failure, fixture.session.state.value.failure)
            assertNull(fixture.session.state.value.delivery)
            assertNull(fixture.session.state.value.opening)
            fixture.preparing.set {}
            fixture.prepare()
            assertNull(fixture.session.state.value.failure)
            assertEquals(2, fixture.created.get())
            assertEquals(0, fixture.sent.get())
        } finally {
            fixture.close()
        }
        assertEquals(1, fixture.closed.get())
    }
}
