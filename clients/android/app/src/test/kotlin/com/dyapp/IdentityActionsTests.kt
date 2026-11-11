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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private class GatedIdentityResolution {
    val entered = CompletableDeferred<Unit>()
    val calls = AtomicInteger()
    private val resume = CountDownLatch(1)

    fun resolve(action: () -> Opened): Shown {
        calls.incrementAndGet()
        entered.complete(Unit)
        check(resume.await(5, TimeUnit.SECONDS)) { "Identity fixture was not released" }
        return Shown(action())
    }

    fun release() = resume.countDown()
}

class IdentityActionsTests {
    @Test
    fun aPendingNativeCallKeepsLoadingAndRejectsDuplicateActions() = runBlocking {
        val fixture = GatedIdentityResolution()
        val controller = IdentityActions(this, fixture::resolve)
        val job = controller.initialize(action = { Opened.Picker })
        try {
            withTimeout(5_000) { fixture.entered.await() }
            assertTrue(controller.state.value.loading)
            assertNull(controller.state.value.shown)
            assertNull(controller.perform(action = { Opened.Setup }))
            assertEquals(1, fixture.calls.get())
        } finally {
            fixture.release()
        }
        withTimeout(5_000) { job.join() }
        assertFalse(controller.state.value.loading)
        assertEquals(1L, controller.state.value.revision)
        assertEquals(Opened.Picker, controller.state.value.shown?.opened)
    }

    @Test
    fun aDisposedOwnerDoesNotPublishTheUninterruptibleWorkersCompletion() = runBlocking {
        val owner = SupervisorJob()
        val scope = CoroutineScope(coroutineContext + owner)
        val fixture = GatedIdentityResolution()
        val controller = IdentityActions(scope, fixture::resolve)
        val callbacks = AtomicInteger()
        val job = controller.initialize({ Opened.Picker }) { callbacks.incrementAndGet() }
        try {
            withTimeout(5_000) { fixture.entered.await() }
            owner.cancel()
        } finally {
            fixture.release()
        }
        withTimeout(5_000) { job.join() }
        assertNull(controller.state.value.shown)
        assertEquals(0, callbacks.get())
        assertEquals(0L, controller.state.value.revision)
    }

    @Test
    fun eachCompletedCommandAdvancesTheSignalEvenWhenItsScreenDoesNotChange() = runBlocking {
        val controller = IdentityActions(this) { action -> Shown(action()) }
        withTimeout(5_000) { controller.initialize(action = { Opened.Picker }).join() }
        val first = controller.state.value.revision
        withTimeout(5_000) { checkNotNull(controller.perform(action = { Opened.Picker })).join() }
        assertEquals(first + 1, controller.state.value.revision)
        assertEquals(Opened.Picker, controller.state.value.shown?.opened)
    }
}
