package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class PeerTransitionTests {
    @Test
    fun readyStateDoesNotTriggerAnotherReceive() {
        var receives = 0
        PeerTransition("already applied", 10) { 0L }.await({ true }) { receives++ }
        assertEquals(0, receives)
    }

    @Test
    fun eachSuccessfulTransitionResetsTheFailureDeadline() {
        var clock = 0L
        val transition = PeerTransition("next message", 10) { clock }
        repeat(2) {
            var applied = false
            transition.await({ applied }) {
                clock += 9
                applied = true
            }
        }
        assertEquals(18L, clock)
    }

    @Test
    fun aLateReceiveCannotPassJustBecauseItEventuallyAppliedTheMessage() {
        var clock = 0L
        var applied = false
        val failure = assertThrows(IllegalStateException::class.java) {
            PeerTransition("edited reply", 10) { clock }.await({ applied }) {
                clock = 10
                applied = true
            }
        }
        assertTrue(failure.message.orEmpty().contains("edited reply"))
    }

    @Test
    fun signalWaitsConsumeOneDeadlineUntilTheExpectedStateIsReady() {
        var clock = 0L
        var signals = 0
        val budgets = mutableListOf<Long>()
        PeerTransition("signalled message", 10) { clock }.awaitSignal({ signals == 3 }) { remaining ->
            budgets += remaining
            clock += 3
            signals++
        }
        assertEquals(listOf(10L, 7L, 4L), budgets)
    }

    @Test
    fun timeSpentReadingHistoryIsRemovedFromTheSignalBudget() {
        var clock = 0L
        var applied = false
        val budgets = mutableListOf<Long>()
        PeerTransition("history read", 10) { clock }.awaitSignal(
            ready = {
                clock += 3
                applied
            },
            advance = {
                budgets += it
                applied = true
            }
        )
        assertEquals(listOf(7L), budgets)
    }

    @Test
    fun receiveAndHistoryFailuresPropagateWithoutRetryMasking() {
        val failure = IllegalStateException("mailbox unavailable")
        val transition = PeerTransition("reply", 10) { 0L }
        var receives = 0
        assertSame(
            failure,
            assertThrows(IllegalStateException::class.java) {
                transition.await({ false }) {
                    receives++
                    throw failure
                }
            }
        )
        assertEquals(1, receives)
        assertSame(
            failure,
            assertThrows(IllegalStateException::class.java) {
                transition.await({ throw failure }) { receives++ }
            }
        )
        assertEquals(1, receives)
    }
}
