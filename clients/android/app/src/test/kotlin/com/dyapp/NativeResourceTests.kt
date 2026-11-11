package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private class TrackedNativeResource : AutoCloseable {
    val closes = AtomicInteger()
    var closedOn: Thread? = null

    override fun close() {
        closedOn = Thread.currentThread()
        closes.incrementAndGet()
    }
}

private class NativeFactoryGate {
    val entered = CompletableDeferred<Unit>()
    val resource = TrackedNativeResource()
    private val released = CountDownLatch(1)

    fun create(): TrackedNativeResource {
        entered.complete(Unit)
        check(released.await(5, TimeUnit.SECONDS)) { "Native factory fixture was not released" }
        return resource
    }

    fun release() = released.countDown()
}

class NativeResourceTests {
    @Test
    fun cancellationDuringAnUninterruptibleFactoryClosesItsLateResourceWithoutPublishing() = runBlocking {
        val gate = NativeFactoryGate()
        val ownerThread = Thread.currentThread()
        val consumers = AtomicInteger()
        val job = launch { withNativeResource(gate::create) { consumers.incrementAndGet() } }
        try {
            withTimeout(5_000) { gate.entered.await() }
            job.cancel()
            assertFalse(job.isCompleted)
            assertEquals(0, gate.resource.closes.get())
        } finally {
            gate.release()
        }
        withTimeout(5_000) { job.join() }
        assertTrue(job.isCancelled)
        assertEquals(0, consumers.get())
        assertEquals(1, gate.resource.closes.get())
        assertNotSame(ownerThread, gate.resource.closedOn)
    }

    @Test
    fun cancellingThePublishedOwnersChildClosesExactlyOnce() = runBlocking {
        val resource = TrackedNativeResource()
        val entered = CompletableDeferred<Unit>()
        val job = launch {
            withNativeResource({ resource }) {
                entered.complete(Unit)
                awaitCancellation()
            }
        }
        withTimeout(5_000) { entered.await() }
        job.cancel()
        withTimeout(5_000) { job.join() }
        assertEquals(1, resource.closes.get())
    }

    @Test
    fun consumerFailurePropagatesAfterClosingOffUiAndTheConsumerKeepsItsCallerContext() = runBlocking {
        val resource = TrackedNativeResource()
        val ownerThread = Thread.currentThread()
        val failure = IllegalStateException("consumer failed")
        val result = runCatching {
            withNativeResource({
                assertNotSame(ownerThread, Thread.currentThread())
                resource
            }) {
                assertSame(ownerThread, Thread.currentThread())
                throw failure
            }
        }
        val thrown = result.exceptionOrNull()
        // Coroutine debug mode may rethrow a stack-recovered copy that wraps the original.
        assertTrue(thrown === failure || thrown?.cause === failure)
        assertEquals(1, resource.closes.get())
        assertNotSame(ownerThread, resource.closedOn)
    }
}
