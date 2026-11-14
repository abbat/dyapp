package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.SearchOutboxWake

/** Framework execution smoke; persistence/network retry policy requires separate runtime coverage. */
class SearchOutboxJobServiceTests {
    @Test
    fun frameworkJobRequestsCompletionAfterTheRealNativePass() {
        SearchOutboxServiceFixture().use { fixture ->
            fixture.schedule()
            fixture.gate.awaitEntered()
            fixture.gate.release()
            val completion = fixture.awaitFinished()
            assertTrue(completion.requested)
            assertFalse(completion.retry)
            assertEquals(SearchOutboxWake.Idle, fixture.wake.get())
            assertEquals(1, fixture.completions.get())
            fixture.requireAdmissionReleased()
        }
    }

    @Test
    fun realNativeStorageFailureRequestsBackoffWithoutReplacingTheDatabase() {
        SearchOutboxServiceFixture().use { fixture ->
            val database = fixture.damageDatabase()
            val original = database.readBytes()
            fixture.schedule()
            fixture.gate.awaitEntered()
            fixture.gate.release()
            val completion = fixture.awaitFinished()
            assertTrue(completion.requested)
            assertTrue(completion.retry)
            assertEquals(SearchOutboxWake.Retry, fixture.wake.get())
            assertTrue(original.contentEquals(database.readBytes()))
            fixture.requireAdmissionReleased()
        }
    }

    @Test
    fun frameworkStopSuppressesCompletionAfterThePausedNativeCallReturns() {
        SearchOutboxServiceFixture().use { fixture ->
            fixture.schedule()
            fixture.gate.awaitEntered()
            fixture.stop()
            fixture.gate.release()
            val completion = fixture.awaitFinished()
            assertFalse(completion.requested)
            assertFalse(completion.retry)
            assertEquals(SearchOutboxWake.Idle, fixture.wake.get())
            assertEquals(0, fixture.completions.get())
            fixture.requireAdmissionReleased()
        }
    }
}
