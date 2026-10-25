package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AppLockLifecycleTests {
    private var clock = 1_000L
    private fun lifecycle(pinEnabled: Boolean) = AppLockLifecycle(pinEnabled) { clock }

    @Test
    fun freshProcessNeedsUnlockAndQuickBackgroundReturnKeepsSession() {
        val lock = lifecycle(true)
        assertTrue(lock.isLocked)
        assertNull(lock.millisUntilLock())
        lock.onUnlocked()
        assertEquals(300_000L, lock.millisUntilLock())
        lock.onBackground()
        assertTrue(lock.snapshotObscured)
        clock += 59_999
        assertFalse(lock.isLocked)
        lock.onForeground()
        assertFalse(lock.snapshotObscured)
        assertFalse(lock.isLocked)
        assertEquals(300_000L, lock.millisUntilLock())
    }

    @Test
    fun backgroundLocksAtSixtySecondsEvenIfInteractionArrivesLate() {
        val lock = lifecycle(true)
        lock.onUnlocked()
        lock.onBackground()
        clock += 60_000
        lock.onInteraction()
        assertTrue(lock.isLocked)
        lock.onForeground()
        assertTrue(lock.isLocked)
        assertNull(lock.millisUntilLock())
    }

    @Test
    fun foregroundInteractionResetsIdleButLateInputCannotReviveSession() {
        val lock = lifecycle(true)
        lock.onUnlocked()
        clock += 299_999
        lock.onInteraction()
        assertEquals(300_000L, lock.millisUntilLock())
        clock += 300_000
        lock.onInteraction()
        assertTrue(lock.isLocked)
    }

    @Test
    fun skippedPinMayBeEnrolledAndCleared() {
        val lock = lifecycle(false)
        assertFalse(lock.isLocked)
        lock.onBackground()
        assertTrue(lock.snapshotObscured)
        clock += 120_000
        lock.onForeground()
        assertFalse(lock.isLocked)
        lock.onPinEnrolled()
        assertTrue(lock.isPinEnabled)
        clock += 300_000
        assertTrue(lock.isLocked)
        lock.onPinCleared()
        assertFalse(lock.isPinEnabled)
        assertFalse(lock.isLocked)
    }
}
