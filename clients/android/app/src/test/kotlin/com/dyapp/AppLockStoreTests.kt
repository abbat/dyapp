package com.dyapp

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.AppLockStorageException
import uniffi.dyapp.KeyStore
import uniffi.dyapp.MobileAppLockEntry
import uniffi.dyapp.WrapException
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.UUID

private class FakeAppLockKey : KeyStore {
    var failWrap = false
    var failUnwrap = false
    var unwraps = 0

    override fun wrap(secret: ByteArray): ByteArray {
        if (failWrap) throw WrapException.Failed("test wrap failure")
        return secret.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
    }

    override fun unwrap(wrapped: ByteArray): ByteArray {
        unwraps++
        if (failUnwrap) throw WrapException.Failed("test unwrap failure")
        return wrap(wrapped)
    }
}

// Counter/mode/authentication regressions moved to core app_lock; these tests cover platform IO only.
class AppLockStoreTests {
    private val root = File("build/app-lock-io-tests/${UUID.randomUUID()}").apply { check(mkdirs()) }
    private val keys = FakeAppLockKey()
    private fun files() = AppLockFiles(root, keys)

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    @Test
    fun opaqueRecordIsProtectedAndCallbackPlaintextIsCleared() {
        val plain = byteArrayOf(10, 20, 30, 40)
        val expected = plain.clone()
        files().writeEntry(MobileAppLockEntry.RECORD, plain)
        assertTrue(plain.all { it == 0.toByte() })
        assertFalse(File(root, "app-lock").readBytes().contentEquals(expected))
        assertTrue(files().readEntry(MobileAppLockEntry.RECORD)!!.contentEquals(expected))
    }

    @Test
    fun onlyConfirmedAbsenceIsNullAndInvalidRootsAreFailures() {
        assertNull(files().readEntry(MobileAppLockEntry.RECORD))
        val notDirectory = File(root, "not-a-directory").apply { writeText("fixture") }
        val invalid = AppLockFiles(notDirectory, keys)
        assertThrows(AppLockStorageException::class.java) { invalid.readEntry(MobileAppLockEntry.RECORD) }
        val missing = AppLockFiles(File(root, "missing-root"), keys)
        val plain = byteArrayOf(1, 2, 3)
        assertThrows(AppLockStorageException::class.java) { missing.writeEntry(MobileAppLockEntry.RECORD, plain) }
        assertTrue(plain.all { it == 0.toByte() })
    }

    @Test
    fun encryptedByteLimitIsInclusiveAndOversizedInputNeverReachesUnwrap() {
        val record = File(root, "app-lock")
        record.writeBytes(ByteArray(256) { 7 })
        assertEquals(256, files().readEntry(MobileAppLockEntry.RECORD)!!.size)
        val before = keys.unwraps
        record.writeBytes(ByteArray(257) { 7 })
        assertThrows(AppLockStorageException::class.java) { files().readEntry(MobileAppLockEntry.RECORD) }
        assertEquals(before, keys.unwraps)
    }

    @Test
    fun recordAndTemporarySymlinksNeverFollowTheirTargets() {
        val target = File(root, "target").apply { writeText("untouched") }
        Files.createSymbolicLink(File(root, "app-lock").toPath(), target.toPath().toAbsolutePath())
        assertThrows(AppLockStorageException::class.java) { files().readEntry(MobileAppLockEntry.RECORD) }
        val atomicRoot = File(root, "atomic").apply { check(mkdir()) }
        Files.createSymbolicLink(File(atomicRoot, "app-lock.tmp").toPath(), target.toPath().toAbsolutePath())
        val atomic = AppLockFiles(atomicRoot, keys)
        assertThrows(
            AppLockStorageException::class.java
        ) { atomic.writeEntry(MobileAppLockEntry.RECORD, byteArrayOf(1)) }
        assertNull(atomic.readEntry(MobileAppLockEntry.RECORD))
        assertEquals("untouched", target.readText())
    }

    @Test
    fun failureBeforeAtomicMoveLeavesThePreviousRecord() {
        val before = byteArrayOf(10, 20, 30)
        files().writeEntry(MobileAppLockEntry.RECORD, before.clone())
        check(File(root, "app-lock.tmp").mkdir())
        val replacement = byteArrayOf(40, 50, 60)
        assertThrows(AppLockStorageException::class.java) { files().writeEntry(MobileAppLockEntry.RECORD, replacement) }
        assertTrue(replacement.all { it == 0.toByte() })
        assertTrue(files().readEntry(MobileAppLockEntry.RECORD)!!.contentEquals(before))
    }

    @Test
    fun unconfirmedMarkerSyncIsFailureAndCannotRemoveTheExistingPinFile() {
        val before = byteArrayOf(10, 20, 30)
        files().writeEntry(MobileAppLockEntry.RECORD, before.clone())
        var reachedSync = false
        val unconfirmed = AppLockFiles(root, keys) { directory ->
            assertEquals(root, directory)
            assertTrue(File(root, AppLockStore.BIOMETRIC).isFile)
            reachedSync = true
            throw IOException("directory sync unconfirmed")
        }
        assertThrows(
            AppLockStorageException::class.java
        ) { unconfirmed.writeEntry(MobileAppLockEntry.BIOMETRIC, byteArrayOf()) }
        assertTrue(reachedSync)
        assertTrue(files().readEntry(MobileAppLockEntry.RECORD)!!.contentEquals(before))
    }

    @Test
    fun keystoreFailuresRemainStructuredAndNeverBecomeAbsence() {
        val plain = byteArrayOf(1, 2, 3)
        keys.failWrap = true
        assertThrows(AppLockStorageException::class.java) { files().writeEntry(MobileAppLockEntry.RECORD, plain) }
        assertTrue(plain.all { it == 0.toByte() })
        keys.failWrap = false
        files().writeEntry(MobileAppLockEntry.RECORD, byteArrayOf(1, 2, 3))
        keys.failUnwrap = true
        assertThrows(AppLockStorageException::class.java) { files().readEntry(MobileAppLockEntry.RECORD) }
    }

    @Test
    fun removalIsIdempotentAndCompletesDirectorySyncBeforeReturning() {
        files().writeEntry(MobileAppLockEntry.RECORD, byteArrayOf(1, 2, 3))
        var syncs = 0
        val tracked = AppLockFiles(root, keys) {
            assertFalse(File(root, "app-lock").exists())
            syncs++
        }
        tracked.removeEntry(MobileAppLockEntry.RECORD)
        assertEquals(1, syncs)
        assertNull(tracked.readEntry(MobileAppLockEntry.RECORD))
        tracked.removeEntry(MobileAppLockEntry.RECORD)
        assertEquals(1, syncs)
    }
}
