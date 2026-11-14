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
            assertTrue(fixture.awaitFinished())
            assertEquals(SearchOutboxWake.Idle, fixture.wake.get())
            assertEquals(1, fixture.completions.get())
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
            assertFalse(fixture.awaitFinished())
            assertEquals(SearchOutboxWake.Idle, fixture.wake.get())
            assertEquals(0, fixture.completions.get())
            fixture.requireAdmissionReleased()
        }
    }
}
