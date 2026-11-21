package com.dyapp

import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatForwardReadCancellationTests {
    @Test
    fun cancelledReadSuccessCannotResetTheReplacementReadOrPublishOldTargets() = runBlocking {
        replaceWhileReading(failed = false)
    }

    @Test
    fun cancelledReadFailureCannotResetTheReplacementReadOrPublishAnError() = runBlocking {
        replaceWhileReading(failed = true)
    }

    private suspend fun replaceWhileReading(failed: Boolean) = coroutineScope {
        val f = ForwardStateFixture()
        val original = f.ask()
        val old = ForwardReadOperation(f.state, original, this)
        var replacement: ForwardReadOperation? = null
        try {
            old.awaitStarted()
            assertTrue(f.state.canCancel)
            f.state.cancel(original)
            assertNull(f.state.opening)
            val current = f.ask()
            val fresh = ForwardReadOperation(f.state, current, this)
            replacement = fresh
            fresh.awaitStarted()
            if (failed) old.fail() else old.complete(listOf("obsolete"))
            requireLoading(f.state, current)
            f.state.cancel(original)
            assertSame(current, f.state.opening)
            fresh.complete(listOf("fresh"))
            assertEquals(listOf("fresh"), f.state.targets)
            assertFalse(f.state.busy)
            assertEquals(0, old.sends.get() + fresh.sends.get())
        } finally {
            replacement?.close()
            old.close()
        }
    }

    private fun requireLoading(state: ChatForwardPresentation, current: ForwardSelection) {
        assertSame(current, state.opening)
        assertTrue(state.busy)
        assertTrue(state.canCancel)
        assertNull(state.targets)
        assertNull(state.chosen)
        assertNull(state.failure)
        assertNull(state.submitted)
    }
}
