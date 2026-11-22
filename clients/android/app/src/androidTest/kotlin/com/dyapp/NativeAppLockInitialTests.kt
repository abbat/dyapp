package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import java.io.File
import java.util.UUID

/** Actual JNI initial admission through separate facades sharing one protected store. */
class NativeAppLockInitialTests {
    private val root = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "native-app-lock-initial-${UUID.randomUUID()}",
    ).apply { check(mkdir()) }
    private val keys = NativeAppLockTestKey()
    private fun store() = AppLockStore(root, keys)

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    @Test
    fun staleInitialChoicesCannotReplaceNewProtectionAcrossFacadeInstances() {
        val stale = store()
        store().useBiometric()
        assertThrows(CoreException::class.java) { stale.enroll("123456") }
        assertThrows(CoreException::class.java) { stale.useBiometric() }
        assertEquals(StoredLock.BIOMETRIC, stale.mode())
        assertFalse(File(root, "app-lock").exists())
        // This fixture explicitly exercises the distinct platform-authorized recovery API.
        store().resetAfterDeviceCredential(null)
        store().enroll("123456")
        val before = File(root, "app-lock").readBytes()
        assertThrows(CoreException::class.java) { stale.enroll("654321") }
        assertThrows(CoreException::class.java) { stale.useBiometric() }
        assertEquals(StoredLock.PIN, stale.mode())
        assertTrue(File(root, "app-lock").readBytes().contentEquals(before))
        assertEquals(PinAttempt.UNLOCKED, stale.attempt("123456"))
        store().useBiometric(afterDeviceCredential = true)
        assertEquals(StoredLock.BIOMETRIC, store().mode())
    }

    @Test
    fun corruptInitialAdmissionNeverOverwritesOrCreatesAModeMarker() {
        val record = File(root, "app-lock")
        record.writeBytes(keys.wrap(byteArrayOf(255.toByte())))
        val before = record.readBytes()
        assertThrows(CoreException::class.java) { store().enroll("123456") }
        assertThrows(CoreException::class.java) { store().useBiometric() }
        assertTrue(record.readBytes().contentEquals(before))
        assertFalse(File(root, AppLockStore.BIOMETRIC).exists())
        assertFalse(File(root, AppLockStore.SKIPPED).exists())
    }
}
