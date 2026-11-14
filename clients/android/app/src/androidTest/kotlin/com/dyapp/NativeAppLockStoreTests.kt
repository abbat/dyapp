package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import uniffi.dyapp.KeyStore
import uniffi.dyapp.MobileAppLock
import uniffi.dyapp.MobileAppLockFormat
import uniffi.dyapp.hashAppPin
import java.io.File
import java.io.IOException
import java.util.UUID

private class NativeAppLockTestKey : KeyStore {
    override fun wrap(secret: ByteArray): ByteArray = secret.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
    override fun unwrap(wrapped: ByteArray): ByteArray = wrap(wrapped)
}

/** Actual JNI/core callbacks in an isolated cache folder; no app-wide lock, identities or node changes. */
class NativeAppLockStoreTests {
    private val root = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "native-app-lock-${UUID.randomUUID()}",
    ).apply { check(mkdir()) }
    private val keys = NativeAppLockTestKey()
    private fun store() = AppLockStore(root, keys)

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    private fun legacy(failures: Int) {
        val verifier = hashAppPin("123456")
        val plain = byteArrayOf(1, failures.toByte()) + verifier
        try {
            File(root, "app-lock").writeBytes(keys.wrap(plain))
        } finally {
            verifier.fill(0)
            plain.fill(0)
        }
    }

    private fun failures(): Int {
        val plain = keys.unwrap(File(root, "app-lock").readBytes())
        return try {
            plain[1].toInt()
        } finally {
            plain.fill(0)
        }
    }

    @Test
    fun oldWrappedRecordWorksAcrossFacadeInstancesAndCurrentPinCanDisableIt() {
        legacy(4)
        assertEquals(StoredLock.PIN, store().mode())
        assertEquals(PinAttempt.UNLOCKED, store().attempt("123456"))
        assertEquals(0, failures())
        assertEquals(PinAttempt.WRONG, store().attempt("654321"))
        assertEquals(1, failures())
        assertEquals(PinAttempt.UNLOCKED, store().change("123456", null))
        assertEquals(StoredLock.OFF, store().mode())
    }

    @Test
    fun legacyFailureLimitRefusesEvenTheCorrectPinWithoutReplacingItsRecord() {
        legacy(5)
        val before = File(root, "app-lock").readBytes()
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().attempt("123456"))
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().change("123456", "654321"))
        assertEquals(5, failures())
        assertTrue(File(root, "app-lock").readBytes().contentEquals(before))
        assertEquals(StoredLock.PIN, store().mode())
    }

    @Test
    fun markerSyncFailureCrossesTheNativeCallbackAndLeavesThePinEffective() {
        legacy(0)
        val before = File(root, "app-lock").readBytes()
        val unconfirmed = AppLockFiles(root, keys) { throw IOException("directory sync unconfirmed") }
        MobileAppLock(MobileAppLockFormat.ANDROID, unconfirmed).use { lock ->
            assertThrows(CoreException::class.java) { lock.useBiometric() }
        }
        assertTrue(File(root, AppLockStore.BIOMETRIC).isFile)
        assertEquals(StoredLock.PIN, store().mode())
        assertTrue(File(root, "app-lock").readBytes().contentEquals(before))
    }

    @Test
    fun malformedWrappedRecordNeverBecomesFreshEnrollment() {
        File(root, "app-lock").writeBytes(keys.wrap(byteArrayOf(1, 2, 3)))
        assertThrows(CoreException::class.java) { store().mode() }
        assertThrows(CoreException::class.java) { store().enroll("123456") }
        assertThrows(CoreException::class.java) { store().attempt("123456") }
    }
}
