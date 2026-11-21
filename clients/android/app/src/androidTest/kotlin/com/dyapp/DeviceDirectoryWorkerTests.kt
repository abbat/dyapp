package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch

class DeviceDirectoryWorkerTests {
    @Test
    fun staleAndFabricatedConfirmationsCannotActOnARefreshedEqualDirectory() = runBlocking {
        val fixture = DeviceDirectoryFixture()
        try {
            val first = fixture.load()
            assertNull(fixture.session.request(first.rows.first()))
            val old = checkNotNull(fixture.session.request(first.rows.last()))
            assertTrue(fixture.session.dismiss(old))
            val current = fixture.load()
            assertNotSame(first.rows.last(), current.rows.last())
            assertNull(fixture.session.request(first.rows.last()))
            val fresh = checkNotNull(fixture.session.request(current.rows.last()))
            assertNull(fixture.session.confirm(old))
            assertFalse(fixture.session.dismiss(old))
            assertNull(fixture.session.confirm(DeviceDisconnectOpening(fresh.target)))
            assertSame(fresh, fixture.session.state.value.confirmation)
            assertEquals(0, fixture.writeCount.get())
        } finally {
            fixture.close()
        }
        assertEquals(1, fixture.closeCount.get())
    }

    @Test
    fun admittedWriteKeepsItsFrozenTargetAfterWaiterCancellationAndRetirement() = runBlocking {
        val fixture = DeviceDirectoryFixture()
        val successor = DeviceDirectoryFixture()
        val entered = CompletableDeferred<ByteArray>()
        val release = CountDownLatch(1)
        fixture.writes.set { key ->
            entered.complete(key.copyOf())
            // Each test transition has its own deadline; finally always releases the native-shaped call.
            release.await()
        }
        try {
            val (opening, expected) = freezeAndMutateSource(fixture)
            val result = checkNotNull(fixture.session.confirm(opening))
            assertArrayEquals(expected, withTimeout(5_000) { entered.await() })
            assertNull(fixture.session.confirm(opening))
            cancelWaiter(result)
            val retired = fixture.session.retire()
            assertEquals(0, fixture.closeCount.get())
            assertNull(fixture.session.refresh(true))
            val successorListing = successor.load()
            release.countDown()
            awaitCompletion(result, retired)
            assertRetainedResult(fixture, successor, successorListing, expected)
        } finally {
            release.countDown()
            fixture.close()
            successor.close()
        }
        assertEquals(1, fixture.closeCount.get())
    }

    private suspend fun freezeAndMutateSource(
        fixture: DeviceDirectoryFixture
    ): Pair<DeviceDisconnectOpening, ByteArray> {
        val listing = fixture.load()
        val opening = checkNotNull(fixture.session.request(listing.rows.last()))
        val expected = fixture.other.copyOf()
        fixture.other[0] = 99
        opening.target.key()[0] = 88
        return opening to expected
    }

    private suspend fun awaitCompletion(result: Deferred<Result<Unit>>, retired: Deferred<Result<Unit>>) {
        withTimeout(5_000) { result.await().getOrThrow() }
        withTimeout(5_000) { retired.await().getOrThrow() }
    }

    private fun assertRetainedResult(
        fixture: DeviceDirectoryFixture,
        successor: DeviceDirectoryFixture,
        listing: DeviceDirectoryListing,
        expected: ByteArray
    ) {
        val completion = checkNotNull(fixture.session.state.value.completion)
        assertTrue(completion.result.isSuccess)
        assertArrayEquals(expected, completion.target.key())
        assertEquals(1, fixture.writeCount.get())
        assertSame(listing, successor.session.state.value.listing)
        assertNull(successor.session.state.value.completion)
        assertEquals(0, successor.closeCount.get())
    }

    private suspend fun cancelWaiter(result: Deferred<Result<Unit>>) = coroutineScope {
        val entered = CompletableDeferred<Unit>()
        val waiter = launch {
            entered.complete(Unit)
            result.await()
        }
        entered.await()
        waiter.cancelAndJoin()
    }
}
