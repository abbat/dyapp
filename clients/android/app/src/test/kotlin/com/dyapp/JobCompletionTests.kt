package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread

class JobCompletionTests {
    @Test
    fun completionIsDeliveredOnceAndStopCannotRedeliverIt() {
        val completion = JobCompletion()
        var calls = 0
        completion.complete { calls += 1 }
        completion.complete { calls += 1 }
        completion.stop()
        completion.complete { calls += 1 }
        assertEquals(1, calls)
    }

    @Test
    fun stoppingDoesNotInterruptTheWorkerAndSuppressesItsLateCompletion() {
        val completion = JobCompletion()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val finished = CountDownLatch(1)
        val calls = AtomicInteger()
        val failure = AtomicReference<Throwable?>()
        val worker = thread {
            entered.countDown()
            try {
                check(release.await(5, TimeUnit.SECONDS))
                completion.complete { calls.incrementAndGet() }
            } catch (error: Throwable) {
                failure.set(error)
            } finally {
                finished.countDown()
            }
        }
        try {
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            completion.stop()
            assertTrue(worker.isAlive)
        } finally {
            release.countDown()
        }
        assertTrue(finished.await(5, TimeUnit.SECONDS))
        assertNull(failure.get())
        assertEquals(0, calls.get())
    }

    @Test
    fun callbackFailureDoesNotAllowDuplicateCompletion() {
        val completion = JobCompletion()
        val failure = IllegalStateException("callback failed")
        val thrown = assertThrows(IllegalStateException::class.java) {
            completion.complete { throw failure }
        }
        assertSame(failure, thrown)
        var calls = 0
        completion.complete { calls += 1 }
        assertEquals(0, calls)
    }
}
