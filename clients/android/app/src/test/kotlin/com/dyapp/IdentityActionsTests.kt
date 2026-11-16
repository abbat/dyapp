package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
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
    private fun ready(id: String, peer: String = id) =
        Opened.Ready(Slot(id, File("presentation-only"), SlotState.READY), peer)

    @Test
    fun cleanupRefreshesOnlyTheMatchingSourceFromACompletedBatch() = runBlocking {
        val opened = ready("original")
        val controller = IdentityActions(this) { Shown(it()) }
        withTimeout(5_000) { controller.initialize({ opened }).join() }
        val calls = AtomicInteger()
        val events = listOf(
            DeviceCleanupEvent("original", "original", "older"),
            DeviceCleanupEvent("original", "original", "latest"),
            DeviceCleanupEvent("other", "other", null)
        )
        withTimeout(5_000) {
            controller.devicesCleaned(events, { Opened.Setup }, { calls.incrementAndGet() })
        }
        assertEquals(Opened.Setup, controller.state.value.shown?.opened)
        assertEquals("latest", controller.state.value.shown?.durabilityDetail)
        assertEquals(1, calls.get())
    }

    @Test
    fun anotherSourceOrOwnerCannotRefreshTheCurrentScreen() = runBlocking {
        val opened = ready("current")
        val controller = IdentityActions(this) { Shown(it()) }
        withTimeout(5_000) { controller.initialize({ opened }).join() }
        withTimeout(5_000) {
            controller.devicesCleaned(
                listOf(DeviceCleanupEvent("old", "old", null), DeviceCleanupEvent("current", "wrong", null)),
                { error("another source requested a refresh") },
                { error("another source completed a refresh") }
            )
        }
        assertEquals(opened, controller.state.value.shown?.opened)
        assertFalse(controller.state.value.loading)
    }

    @Test
    fun cleanupWaitsForAnAdmittedSwitchAndDoesNotReplaceItsNewIdentity() = runBlocking {
        val fixture = GatedIdentityResolution()
        val gate = AtomicBoolean(false)
        val controller =
            IdentityActions(this) { action -> if (gate.get()) fixture.resolve(action) else Shown(action()) }
        withTimeout(5_000) { controller.initialize({ ready("old") }).join() }
        gate.set(true)
        val other = ready("new")
        val switch = checkNotNull(controller.perform({ other }))
        val refresh = launch(start = CoroutineStart.UNDISPATCHED) {
            controller.devicesCleaned(
                listOf(DeviceCleanupEvent("old", "old", null)),
                { error("late cleanup replaced the new identity") },
                { error("late cleanup published a screen") }
            )
        }
        try {
            withTimeout(5_000) { fixture.entered.await() }
            assertFalse(refresh.isCompleted)
        } finally {
            fixture.release()
        }
        withTimeout(5_000) { switch.join() }
        withTimeout(5_000) { refresh.join() }
        assertEquals(other, controller.state.value.shown?.opened)
        assertEquals(1, fixture.calls.get())
    }

    @Test
    fun repeatedCompletionsKeepOnlyTheLatestForEachSlotAndOwner() {
        val before = DeviceCleanupEvents.events.value
        val opened = ready("coalescing")
        DeviceCleanupEvents.completed(opened, "older")
        val observed = DeviceCleanupEvents.events.value.last()
        try {
            repeat(3) { DeviceCleanupEvents.completed(opened, "latest") }
            DeviceCleanupEvents.completed(ready(opened.slot.id, "another-owner"), null)
            DeviceCleanupEvents.observed(listOf(observed))
            val pending = DeviceCleanupEvents.events.value.filter { it.slotId == opened.slot.id }
            assertEquals(2, pending.size)
            assertEquals("latest", pending.single { it.peerId == opened.peerId }.durabilityDetail)
        } finally {
            DeviceCleanupEvents.observed(DeviceCleanupEvents.events.value.filter { it.slotId == opened.slot.id })
        }
        assertEquals(before, DeviceCleanupEvents.events.value)
    }

    @Test
    fun observingACompletedBatchRetainsNewerQueuedCompletions() {
        val before = DeviceCleanupEvents.events.value
        DeviceCleanupEvents.completed(ready("first"), null)
        val first = DeviceCleanupEvents.events.value.last()
        DeviceCleanupEvents.completed(ready("second"), null)
        val second = DeviceCleanupEvents.events.value.last()
        try {
            DeviceCleanupEvents.observed(listOf(first))
            assertFalse(first in DeviceCleanupEvents.events.value)
            assertTrue(second in DeviceCleanupEvents.events.value)
        } finally {
            DeviceCleanupEvents.observed(listOf(first, second))
        }
        assertEquals(before, DeviceCleanupEvents.events.value)
    }

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
