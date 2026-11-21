package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.MobileLinkFailure
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileQrMatrix
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

private class GatedLinkReceiver {
    val phase = AtomicReference(MobileLinkPhase.WAITING)
    val entered = CompletableDeferred<Unit>()
    val release = CountDownLatch(1)
    val polls = AtomicInteger()
    val closes = AtomicInteger()
    val cancels = AtomicInteger()
    val receipt = MobileLinkReceipt.Failed(MobileLinkFailure.Destination("original-reservation", "disk"))
    val calls = LinkReceiverCalls(
        LinkReceiverDisplay("original-public-code", 123uL, MobileQrMatrix(1, listOf(true))),
        { phase.get() },
        {
            cancels.incrementAndGet()
            phase.compareAndSet(MobileLinkPhase.WAITING, MobileLinkPhase.CANCELLED)
            phase.get()
        },
        {
            polls.incrementAndGet()
            phase.set(MobileLinkPhase.INSTALLING)
            entered.complete(Unit)
            check(release.await(5, TimeUnit.SECONDS)) { "Controlled native poll was not released" }
            phase.set(MobileLinkPhase.FAILED)
            phase.get()
        },
        { if (phase.get() == MobileLinkPhase.FAILED) receipt else null },
        { closes.incrementAndGet() }
    )
}

class LinkReceiverSessionTests {
    @Test
    fun retirementDuringFactoryAdoptsAndCancelsItsActualHandleWithoutPolling() = runBlocking {
        val fixture = GatedLinkReceiver()
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val owner = LinkReceiverSession {
            entered.complete(Unit)
            check(release.await(5, TimeUnit.SECONDS)) { "Controlled native factory was not released" }
            fixture.calls
        }
        assertTrue(owner.start())
        try {
            withTimeout(5_000) { entered.await() }
            owner.retire()
            assertFalse(owner.close())
            assertFalse(owner.start())
        } finally {
            release.countDown()
            withTimeout(5_000) { owner.state.first { !it.busy } }
        }
        assertEquals(MobileLinkPhase.CANCELLED, owner.state.value.phase)
        assertSame(fixture.calls.display, owner.state.value.display)
        assertEquals(1, fixture.cancels.get())
        assertEquals(0, fixture.polls.get())
        assertFalse(owner.poll())
        assertEquals(0, fixture.closes.get())
        assertTrue(owner.close())
        assertFalse(owner.close())
        assertEquals(1, fixture.closes.get())
    }

    @Test
    fun retirementAfterAdmissionRetainsTheExactOriginalReceiptUntilDisposition() = runBlocking {
        val fixture = GatedLinkReceiver()
        val owner = LinkReceiverSession { fixture.calls }
        assertTrue(owner.start())
        withTimeout(5_000) { owner.state.first { !it.busy } }
        assertTrue(owner.poll())
        try {
            withTimeout(5_000) { fixture.entered.await() }
            assertFalse(owner.poll())
            assertFalse(owner.close())
            owner.retire()
            owner.retire()
            assertEquals(MobileLinkPhase.INSTALLING, owner.state.value.phase)
            assertTrue(owner.state.value.busy)
            assertEquals(0, fixture.closes.get())
        } finally {
            fixture.release.countDown()
            withTimeout(5_000) { owner.state.first { !it.busy } }
        }
        assertRetainedFailure(owner, fixture)
        assertTrue(owner.close())
        assertSame(fixture.receipt, owner.state.value.receipt)
        assertEquals(1, fixture.closes.get())
    }

    private fun assertRetainedFailure(owner: LinkReceiverSession, fixture: GatedLinkReceiver) {
        assertTrue(owner.state.value.retired)
        assertFalse(owner.state.value.busy)
        assertEquals(MobileLinkPhase.FAILED, owner.state.value.phase)
        assertSame(fixture.receipt, owner.state.value.receipt)
        assertNull(owner.state.value.error)
        assertFalse(owner.poll())
        assertFalse(owner.start())
        assertEquals(1, fixture.polls.get())
        assertEquals(1, fixture.cancels.get())
    }

    @Test
    fun aRetiredOpeningCannotStartItsFactory() = runBlocking {
        val owner = LinkReceiverSession { error("Retired opening created a native invitation") }
        owner.retire()
        assertFalse(owner.start())
        assertFalse(owner.poll())
        assertNull(owner.state.value.display)
        assertTrue(owner.close())
    }

    @Test
    fun factoryFailureRemainsVisibleAndCannotImplicitlyCreateAnotherDestination() = runBlocking {
        val failure = IllegalStateException("controlled factory refusal")
        val creates = AtomicInteger()
        val owner = LinkReceiverSession {
            creates.incrementAndGet()
            throw failure
        }
        assertTrue(owner.start())
        withTimeout(5_000) { owner.state.first { !it.busy } }
        assertSame(failure, owner.state.value.error)
        assertFalse(owner.state.value.busy)
        assertNull(owner.state.value.receipt)
        assertFalse(owner.start())
        assertFalse(owner.poll())
        assertEquals(1, creates.get())
        assertTrue(owner.close())
    }
}
