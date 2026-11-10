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
