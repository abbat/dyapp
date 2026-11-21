package com.dyapp

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class DeviceDirectoryFailureTests {
    @Test
    fun partialWriteAndCacheFailureRemainSeparateUntilAnotherExplicitAction() = runBlocking {
        val fixture = DeviceDirectoryFixture()
        try {
            val listing = fixture.load()
            val opening = checkNotNull(fixture.session.request(listing.rows.last()))
            val partial = IllegalStateException("Publication refused after mailbox closure")
            val cache = IllegalStateException("Cache unavailable")
            fixture.writes.set { throw partial }
            fixture.reads.set { throw cache }
            val result = withTimeout(5_000) { checkNotNull(fixture.session.confirm(opening)).await() }
            assertSame(partial, result.exceptionOrNull())
            assertPartialFailure(fixture.session.state.value, partial, cache, opening)
            assertEquals(1, fixture.writeCount.get())
            fixture.reads.set { fixture.snapshot }
            val refreshed = fixture.load()
            assertSame(partial, fixture.session.state.value.completion?.result?.exceptionOrNull())
            assertNull(fixture.session.confirm(opening))
            fixture.writes.set {}
            val fresh = checkNotNull(fixture.session.request(refreshed.rows.last()))
            withTimeout(5_000) { checkNotNull(fixture.session.confirm(fresh)).await().getOrThrow() }
            assertEquals(2, fixture.writeCount.get())
            assertTrue(checkNotNull(fixture.session.state.value.completion).result.isSuccess)
            assertNull(fixture.session.state.value.failure)
        } finally {
            fixture.close()
        }
    }

    private fun assertPartialFailure(
        state: DeviceDirectoryViewState,
        partial: Throwable,
        cache: Throwable,
        opening: DeviceDisconnectOpening
    ) {
        assertSame(partial, state.completion?.result?.exceptionOrNull())
        assertSame(cache, state.failure)
        assertSame(opening, state.confirmation)
    }

    @Test
    fun cacheFailureCannotChangeAnAcknowledgedWriteIntoFailedRevocation() = runBlocking {
        val fixture = DeviceDirectoryFixture()
        try {
            val listing = fixture.load()
            val opening = checkNotNull(fixture.session.request(listing.rows.last()))
            val cache = IllegalStateException("Readback unavailable after acknowledged write")
            fixture.reads.set { throw cache }
            withTimeout(5_000) { checkNotNull(fixture.session.confirm(opening)).await().getOrThrow() }
            val state = fixture.session.state.value
            assertTrue(checkNotNull(state.completion).result.isSuccess)
            assertSame(cache, state.failure)
            assertSame(listing, state.listing)
            assertNull(state.confirmation)
            assertEquals(1, fixture.writeCount.get())
        } finally {
            fixture.close()
        }
    }
}
