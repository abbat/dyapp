package com.dyapp

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException

class AppLockLifecycleTests {
    private var clock = 1_000L
    private val timers = mutableListOf<AppLockLifecycle>()
    private fun lifecycle(pinEnabled: Boolean) = AppLockLifecycle(pinEnabled) { clock }.also { timers += it }

    @After
    fun closeTimers() {
        timers.forEach(AppLockLifecycle::close)
    }

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
    fun backwardsClockCoversProtectedContentAndNegativePlatformTimeIsRejected() {
        val lock = lifecycle(true)
        lock.onUnlocked()
        clock = 999
        assertTrue(lock.isLocked)
        assertNull(lock.millisUntilLock())
        clock = -1
        assertThrows(IllegalStateException::class.java) { lifecycle(true) }
    }

    @Test
    fun nativeUnlockRefusesDisabledAndBackgroundSessions() {
        val disabled = lifecycle(false)
        assertThrows(CoreException::class.java) { disabled.onUnlocked() }
        val lock = lifecycle(true)
        lock.onUnlocked()
        lock.onBackground()
        clock += 60_000
        assertThrows(CoreException::class.java) { lock.onUnlocked() }
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
