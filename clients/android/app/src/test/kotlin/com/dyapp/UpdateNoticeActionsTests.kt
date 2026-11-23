package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateNoticeAction
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private class GatedUpdateRead(private val snapshot: MobileUpdateState) {
    val entered = CompletableDeferred<Thread>()
    private val release = CountDownLatch(1)

    fun read(): MobileUpdateState {
        entered.complete(Thread.currentThread())
        check(release.await(5, TimeUnit.SECONDS)) { "Update fixture was not released" }
        return snapshot
    }

    fun complete() = release.countDown()
}

class UpdateNoticeActionsTests {
    private fun offer(version: String = "0.0.2") = MobileUpdateOffer(
        version,
        MobileUpdateDelivery.PACKAGE,
        "https://github.com/abbat/dyapp/releases/tag/v$version",
    )

    private fun snapshot(notice: MobileUpdateOffer? = offer()) = MobileUpdateState(
        installedVersion = "0.0.1",
        interval = MobileUpdateInterval.DAY,
        lastAttempt = 1uL,
        lastSuccess = 1uL,
        inFlight = false,
        availability = MobileUpdateAvailability.AVAILABLE,
        offer = offer(),
        notice = notice,
    )

    @Test
    fun pendingReadRunsOffCallerAndDoesNotFabricateAnOffer() = runBlocking {
        val fixture = GatedUpdateRead(snapshot())
        val actions = UpdateNoticeActions(this, fixture::read) { error("Unexpected dismissal") }
        val caller = Thread.currentThread()
        val job = checkNotNull(actions.initialize())
        try {
            val worker = withTimeout(5_000) { fixture.entered.await() }
            assertTrue(worker !== caller)
            assertTrue(actions.state.value.pending)
            assertNull(actions.state.value.snapshot)
            assertNull(actions.initialize())
            assertNull(actions.later(offer()))
        } finally {
            fixture.complete()
        }
        withTimeout(5_000) { job.join() }
        assertEquals(snapshot(), actions.state.value.snapshot)
        assertFalse(actions.state.value.pending)
    }

    @Test
    fun dismissalWaitsForNativeCommitAndRejectsDuplicateOrStaleTaps() = runBlocking {
        val fixture = GatedUpdateRead(snapshot(null))
        val calls = AtomicInteger()
        val actions = UpdateNoticeActions(this, { snapshot() }) { version ->
            assertEquals("0.0.2", version)
            calls.incrementAndGet()
            MobileUpdateNoticeAction(fixture.read(), true)
        }
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        assertNull(actions.later(offer("0.0.3")))
        val job = checkNotNull(actions.later(offer()))
        try {
            withTimeout(5_000) { fixture.entered.await() }
            assertEquals(offer(), actions.state.value.snapshot?.notice)
            assertTrue(actions.state.value.pending)
            assertNull(actions.later(offer()))
            assertEquals(1, calls.get())
        } finally {
            fixture.complete()
        }
        withTimeout(5_000) { job.join() }
        assertNull(actions.state.value.snapshot?.notice)
        assertEquals(offer(), actions.state.value.snapshot?.offer)
        assertFalse(actions.state.value.pending)
    }

    @Test
    fun failedWriteRetainsTheNoticeAndAllowsExplicitRetry() = runBlocking {
        val calls = AtomicInteger()
        val actions = UpdateNoticeActions(this, { snapshot() }) {
            if (calls.incrementAndGet() == 1) error("Uncommitted write")
            MobileUpdateNoticeAction(snapshot(null), true)
        }
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        withTimeout(5_000) { checkNotNull(actions.later(offer())).join() }
        assertTrue(actions.state.value.failed)
        assertEquals(snapshot(), actions.state.value.snapshot)
        withTimeout(5_000) { checkNotNull(actions.later(offer())).join() }
        assertFalse(actions.state.value.failed)
        assertNull(actions.state.value.snapshot?.notice)
        assertEquals(2, calls.get())
    }

    @Test
    fun staleNativeReplyKeepsItsNewerNoticeWithoutLocalVersionPolicy() = runBlocking {
        val newer = snapshot(offer("0.0.3")).copy(offer = offer("0.0.3"))
        val actions = UpdateNoticeActions(this, { snapshot() }) { MobileUpdateNoticeAction(newer, false) }
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        withTimeout(5_000) { checkNotNull(actions.later(offer())).join() }
        assertEquals(newer, actions.state.value.snapshot)
        assertNull(actions.later(offer()))
    }

    @Test
    fun disposedHostDiscardsCompletionAfterNativeWorkReturns() = runBlocking {
        val owner = SupervisorJob()
        val fixture = GatedUpdateRead(snapshot(null))
        val actions = UpdateNoticeActions(CoroutineScope(coroutineContext + owner), { snapshot() }) {
            MobileUpdateNoticeAction(fixture.read(), true)
        }
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        val job = checkNotNull(actions.later(offer()))
        try {
            withTimeout(5_000) { fixture.entered.await() }
            owner.cancel()
        } finally {
            fixture.complete()
        }
        withTimeout(5_000) { job.join() }
        assertEquals(offer(), actions.state.value.snapshot?.notice)
        assertTrue(actions.state.value.pending)
    }

    @Test
    fun failedInitialReadShowsFailureAndCanBeRetriedWithoutInventingDefaults() = runBlocking {
        val calls = AtomicInteger()
        val actions = UpdateNoticeActions(this, {
            if (calls.incrementAndGet() == 1) error("Unreadable record")
            snapshot(null)
        }) { error("Unexpected dismissal") }
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        assertTrue(actions.state.value.failed)
        assertNull(actions.state.value.snapshot)
        assertFalse(actions.state.value.pending)
        withTimeout(5_000) { checkNotNull(actions.initialize()).join() }
        assertFalse(actions.state.value.failed)
        assertEquals(snapshot(null), actions.state.value.snapshot)
    }
}
