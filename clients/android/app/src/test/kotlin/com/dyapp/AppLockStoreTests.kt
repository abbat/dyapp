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
        assertFalse(lock.hasPin())
        lock.enroll("123456")
        assertTrue(store().hasPin())
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
        assertTrue(runCatching { store().hasPin() }.isFailure)
        assertTrue(runCatching { store().enroll("654321") }.isFailure)
        assertTrue(runCatching { store().attempt("123456") }.isFailure)
    }

    @Test
    fun clearRequiresExplicitRecoveryCall() {
        store().enroll("123456")
        store().resetAfterDeviceCredential(null)
        assertFalse(store().hasPin())
        assertTrue(store().setupDone())
    }

    @Test
    fun setupIsDoneAfterSkipOrEnrollmentOnly() {
        assertFalse(store().setupDone())
        store().skip()
        assertTrue(store().setupDone())
        assertFalse(store().hasPin())
        File(root, AppLockStore.SKIPPED).delete()
        store().enroll("123456")
        assertTrue(store().setupDone())
    }
}
