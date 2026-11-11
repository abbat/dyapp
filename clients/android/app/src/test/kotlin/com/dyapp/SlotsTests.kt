package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.dyapp.MobileSlotRecovery
import java.io.File

class SlotsTests {
    @Test
    fun aMissingPointerTargetNeverInventsALocationOrBecomesSetup() {
        val id = "11111111-1111-4111-8111-111111111111"
        val recovery = recoveryView(id, MobileSlotRecovery.MISSING, null, emptyList())
        assertEquals(id, recovery.id)
        assertNull(recovery.slot)
        assertEquals("missing", recovery.reason)
        assertFalse(removable(recovery))
    }

    @Test
    fun anInvalidPointerRetainsItsNativeRecoveryReasonWithoutALocation() {
        val recovery = recoveryView(null, MobileSlotRecovery.INVALID_POINTER, null, emptyList())
        assertNull(recovery.id)
        assertNull(recovery.slot)
        assertEquals("invalidPointer", recovery.reason)
        assertFalse(removable(recovery))
    }

    @Test
    fun recoveryUsesOnlyTheInventoriedCandidateAndPreservesTheKnownPeer() {
        val slot = Slot("candidate", File("native-inventory-location"), SlotState.PENDING)
        val recovery = recoveryView(slot.id, MobileSlotRecovery.INCOMPLETE, "peer", listOf(slot))
        assertSame(slot, recovery.slot)
        assertEquals("peer", recovery.peerId)
        assertEquals("incomplete", recovery.reason)
    }

    @Test
    fun allNativeRecoveryReasonsProjectToStableExistingLabels() {
        val labels = mapOf(
            MobileSlotRecovery.MISSING to "missing",
            MobileSlotRecovery.UNAVAILABLE to "unavailable",
            MobileSlotRecovery.INCOMPLETE to "incomplete",
            MobileSlotRecovery.DELETING to "deleting",
            MobileSlotRecovery.DAMAGED to "damaged",
            MobileSlotRecovery.INVALID_POINTER to "invalidPointer"
        )
        for ((reason, text) in labels) {
            assertEquals(text, recoveryView(null, reason, null, emptyList()).reason)
        }
    }

    @Test
    fun removalNeedsAKnownNonLegacyCandidateAndAnExistingRecoveryAction() {
        val legacy = Slot(Slots.LEGACY, File("legacy"), SlotState.READY)
        val pending = Slot("candidate", File("candidate"), SlotState.PENDING)
        assertFalse(removable(Opened.Ready(legacy, "peer")))
        assertFalse(removable(Opened.Recovery(pending, "damaged", null)))
        assertEquals(true, removable(Opened.Recovery(pending, "incomplete", null)))
    }
}
