package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class PrivacyPreferenceStateTests {
    @Test
    fun unknownValueCannotAuthorizeAWriteBeforeSuccessfulReadRetry() {
        val state = PrivacyPreferenceState()
        assertFalse(state.begin(true, state.revision))
        state.complete(Result.failure(IllegalStateException("read failed")))
        assertNull(state.value)
        assertFalse(state.editable())
        assertFalse(state.begin(false, state.revision))
        assertTrue(state.begin(null, state.revision))
        state.complete(Result.success(false))
        assertEquals(false, state.value)
        assertTrue(state.editable())
    }

    @Test
    fun pendingAdmissionAndRetiredSnapshotCannotSubmitTwice() {
        val state = PrivacyPreferenceState()
        state.complete(Result.success(false))
        val shown = state.revision
        assertTrue(state.begin(true, shown))
        assertFalse(state.begin(true, shown))
        assertFalse(state.begin(null, shown))
        state.complete(Result.success(true))
        assertFalse(state.begin(false, shown))
        assertTrue(state.begin(false, state.revision))
    }

    @Test
    fun uncertainWriteKeepsKnownValueButOnlyReadbackCanReenableMutation() {
        val state = PrivacyPreferenceState()
        state.complete(Result.success(true))
        assertTrue(state.begin(false, state.revision))
        state.complete(Result.failure(IllegalStateException("unconfirmed write")))
        assertEquals(true, state.value)
        assertFalse(state.editable())
        assertFalse(state.begin(false, state.revision))
        assertTrue(state.begin(null, state.revision))
        state.complete(Result.success(false))
        assertEquals(false, state.value)
        assertTrue(state.editable())
    }

    @Test
    fun ownerRetirementRejectsBothOldCallbacksAndLateCompletion() {
        val state = PrivacyPreferenceState()
        state.complete(Result.success(false))
        val shown = state.revision
        assertTrue(state.begin(true, shown))
        state.retire()
        state.complete(Result.success(true))
        assertFalse(state.editable())
        assertFalse(state.begin(true, shown))
        assertFalse(state.begin(null, shown))
        assertEquals(false, state.value)
        assertSame(shown, state.revision)
    }

    @Test
    fun equalFreshReadStillRetiresTheOldConsentSnapshot() {
        val state = PrivacyPreferenceState()
        state.complete(Result.success(true))
        val shown = state.revision
        assertTrue(state.begin(null, shown))
        state.complete(Result.success(true))
        assertNotSame(shown, state.revision)
        assertFalse(state.begin(false, shown))
        assertTrue(state.begin(false, state.revision))
    }
}
