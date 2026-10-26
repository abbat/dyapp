package com.dyapp

import java.io.File
import java.nio.file.Files
import java.security.MessageDigest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.KeyStore

private class FakeAppLockKey : KeyStore {
    override fun wrap(secret: ByteArray): ByteArray = secret.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
    override fun unwrap(wrapped: ByteArray): ByteArray = wrap(wrapped)
}

private class FakePinVerifier : AppPinVerifier {
    override fun hash(pin: String): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256").digest(pin.toByteArray())
        return ByteArray(49) { if (it == 0) 1.toByte() else digest[it % digest.size] }
    }

    override fun verify(pin: String, record: ByteArray): Boolean = hash(pin).contentEquals(record)
}

class AppLockStoreTests {
    private val root = Files.createTempDirectory("dyapp-app-lock-test").toFile()
    private val keys = FakeAppLockKey()
    private val verifier = FakePinVerifier()
    private fun store() = AppLockStore(root, keys, verifier)

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    @Test
    fun persistsFailuresAcrossRestartsAndRequiresCredentialAfterFive() {
        val lock = store()
        assertFalse(lock.mode() == StoredLock.PIN)
        lock.enroll("123456")
        assertTrue(store().mode() == StoredLock.PIN)
        assertFalse(String(File(root, "app-lock").readBytes()).contains("123456"))
        repeat(4) { assertEquals(PinAttempt.WRONG, store().attempt("654321")) }
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().attempt("654321"))
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().attempt("123456"))
        store().resetAfterDeviceCredential("123456")
        assertEquals(PinAttempt.UNLOCKED, store().attempt("123456"))
        assertEquals(PinAttempt.WRONG, store().attempt("654321"))
    }

    @Test
    fun corruptRecordNeverBecomesFreshEnrollment() {
        store().enroll("123456")
        File(root, "app-lock").writeBytes(byteArrayOf(1, 2, 3))
        assertTrue(runCatching { store().mode() == StoredLock.PIN }.isFailure)
        assertTrue(runCatching { store().enroll("654321") }.isFailure)
        assertTrue(runCatching { store().attempt("123456") }.isFailure)
    }

    @Test
    fun clearRequiresExplicitRecoveryCall() {
        store().enroll("123456")
        store().resetAfterDeviceCredential(null)
        assertFalse(store().mode() == StoredLock.PIN)
        assertTrue(store().mode() != StoredLock.NEW)
    }

    @Test
    fun changeNeedsTheCurrentPinAndCountsAWrongOne() {
        store().enroll("123456")
        assertEquals(PinAttempt.WRONG, store().change("654321", "111111"))
        assertEquals(PinAttempt.UNLOCKED, store().change("123456", "111111"))
        assertEquals(PinAttempt.WRONG, store().attempt("123456"))
        assertEquals(PinAttempt.UNLOCKED, store().change("111111", null))
        assertFalse(store().mode() == StoredLock.PIN)
        assertTrue(store().mode() != StoredLock.NEW)
        store().enroll("222222")
        repeat(4) { store().attempt("654321") }
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().change("654321", null))
        assertEquals(PinAttempt.DEVICE_CREDENTIAL_REQUIRED, store().change("222222", null))
        assertTrue(store().mode() == StoredLock.PIN)
    }

    @Test
    fun biometricModeReplacesThePinAndBack() {
        store().enroll("123456")
        store().useBiometric()
        assertTrue(store().mode() == StoredLock.BIOMETRIC)
        assertFalse(store().mode() == StoredLock.PIN)
        store().resetAfterDeviceCredential("111111")
        assertFalse(store().mode() == StoredLock.BIOMETRIC)
        assertEquals(PinAttempt.UNLOCKED, store().attempt("111111"))
        store().resetAfterDeviceCredential(null)
        store().useBiometric()
        store().resetAfterDeviceCredential(null)
        assertEquals(StoredLock.OFF, store().mode())
        assertTrue(runCatching { store().resetAfterDeviceCredential(null) }.isFailure)
    }

    @Test
    fun setupIsDoneAfterSkipOrEnrollmentOnly() {
        assertFalse(store().mode() != StoredLock.NEW)
        store().skip()
        assertTrue(store().mode() != StoredLock.NEW)
        assertFalse(store().mode() == StoredLock.PIN)
        File(root, AppLockStore.SKIPPED).delete()
        store().enroll("123456")
        assertTrue(store().mode() != StoredLock.NEW)
    }
}
