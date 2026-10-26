package com.dyapp

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import uniffi.dyapp.KeyStore
import uniffi.dyapp.hashAppPin
import uniffi.dyapp.verifyAppPin

/** PIN work is supplied by the shared Rust FFI; JVM tests use a deterministic fake. */
interface AppPinVerifier {
    /** Create the stored verifier record for [pin]. */
    fun hash(pin: String): ByteArray

    /** Return whether [pin] matches the stored [record]. */
    fun verify(pin: String, record: ByteArray): Boolean
}

/** Shared Argon2id implementation from the Rust UniFFI library. */
object RustAppPinVerifier : AppPinVerifier {
    override fun hash(pin: String): ByteArray = hashAppPin(pin)
    override fun verify(pin: String, record: ByteArray): Boolean = verifyAppPin(pin, record)
}

/** The result of one PIN attempt against the app-wide lock. */
enum class PinAttempt {
    /** The PIN matched and the failure counter was cleared. */
    UNLOCKED,

    /** The PIN was wrong, but another attempt is allowed. */
    WRONG,

    /** The failure limit was reached; OS authentication is required. */
    DEVICE_CREDENTIAL_REQUIRED,
}

/** The saved app-lock choice: either a PIN or device biometrics, never both. */
enum class StoredLock {
    /** First launch: neither Skip nor a lock has been chosen. */
    NEW,

    /** Skipped or turned off. */
    OFF,

    /** The app unlocks with its PIN. */
    PIN,

    /** The app unlocks with device biometrics or the OS screen lock. */
    BIOMETRIC,
}

/**
 * App-wide protected PIN record and failure counter, independent of identity slots. The UI must
 * authenticate with the OS before calling [resetAfterDeviceCredential]. Every attempted PIN is
 * persisted before KDF work, so a process crash cannot erase a failed attempt.
 */
class AppLockStore(private val root: File, private val keys: KeyStore, private val pin: AppPinVerifier) {
    /** App-wide storage constants and the device-backed store factory. */
    companion object {
        /** Keystore alias shared by all identity slots for the app lock. */
        const val ALIAS = "dyapp-app-lock"

        /** Marker of the first-launch Skip choice; it holds no data. */
        const val SKIPPED = "app-lock-skipped"

        /** Marker of the biometric mode, which replaces the PIN; it holds no data. */
        const val BIOMETRIC = "app-lock-biometric"

        /** Use one app-wide Keystore key, never an identity slot alias. */
        fun onDevice(root: File) = AppLockStore(root, PlatformKeyStore(ALIAS), RustAppPinVerifier)

        private const val VERSION: Byte = 1
        private const val TAG_RECORD_SIZE = 49
        private const val PLAIN_SIZE = 2 + TAG_RECORD_SIZE
        private const val MAX_FAILURES = 5
        private const val CLEAR_FAILED = "Cannot clear app PIN"
        private val ioLock = Any()
    }

    private val file = File(root, "app-lock")
    private val skipped = File(root, SKIPPED)
    private val biometric = File(root, BIOMETRIC)

    private data class State(val failures: Int, val record: ByteArray)

    /**
     * The saved choice; a corrupt record throws and must keep the app locked. A PIN left by a
     * crash during a switch wins over the biometric marker, so the app stays locked.
     */
    fun mode(): StoredLock = synchronized(ioLock) {
        when {
            read() != null -> StoredLock.PIN
            biometric.exists() -> StoredLock.BIOMETRIC
            skipped.exists() -> StoredLock.OFF
            else -> StoredLock.NEW
        }
    }

    /**
     * Replace any PIN with the biometric mode. Call on first launch, with the lock off, or after
     * a fresh OS device-credential check; the marker comes first so a crash never unlocks.
     */
    fun useBiometric() = synchronized(ioLock) {
        biometric.createNewFile()
        if (read() != null) check(file.delete()) { CLEAR_FAILED }
    }

    /** Record the first-launch Skip choice; it creates no PIN and leaves an existing one. */
    fun skip() = synchronized(ioLock) { skipped.createNewFile() }

    /** Enrol on a fresh install only. A replacement requires [resetAfterDeviceCredential]. */
    fun enroll(pinValue: String) = synchronized(ioLock) {
        check(read() == null) { "App PIN already exists" }
        write(State(0, pin.hash(pinValue)))
    }

    /** Consume and persist one attempt before verifying. Five failures require OS authentication. */
    fun attempt(pinValue: String): PinAttempt = synchronized(ioLock) {
        val state = read() ?: throw IOException("App PIN is not enrolled")
        if (state.failures >= MAX_FAILURES) return@synchronized PinAttempt.DEVICE_CREDENTIAL_REQUIRED
        val failures = state.failures + 1
        write(state.copy(failures = failures))
        if (pin.verify(pinValue, state.record)) {
            write(state.copy(failures = 0))
            return@synchronized PinAttempt.UNLOCKED
        }
        if (failures == MAX_FAILURES) PinAttempt.DEVICE_CREDENTIAL_REQUIRED else PinAttempt.WRONG
    }

    /** Settings: replace the PIN, or clear it when [newPin] is null, only if [current] verifies. */
    fun change(current: String, newPin: String?): PinAttempt = synchronized(ioLock) {
        attempt(current).also { if (it == PinAttempt.UNLOCKED) resetAfterDeviceCredential(newPin) }
    }

    /**
     * Call only after a fresh OS device-credential or biometric check; it replaces the PIN or the
     * biometric mode with [newPin], or turns the lock off when it is null.
     */
    fun resetAfterDeviceCredential(newPin: String?) = synchronized(ioLock) {
        val pinSet = read() != null
        check(pinSet || biometric.exists()) { "App lock is not enabled" }
        if (newPin == null) {
            check(!pinSet || file.delete()) { CLEAR_FAILED }
            skipped.createNewFile() // Clearing is a choice too; first-launch setup stays done.
        } else {
            write(State(0, pin.hash(newPin)))
        }
        check(!biometric.exists() || biometric.delete()) { "Cannot clear biometric mode" }
    }

    private fun read(): State? {
        if (!Files.exists(file.toPath(), LinkOption.NOFOLLOW_LINKS)) return null
        if (!Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS) || file.length() > 256) {
            throw IOException("Invalid app-lock file")
        }
        val plain = keys.unwrap(file.readBytes())
        try {
            if (plain.size != PLAIN_SIZE || plain[0] != VERSION || plain[1].toInt() !in 0..MAX_FAILURES || plain[2] != 1.toByte()) {
                throw IOException("Invalid app-lock record")
            }
            return State(plain[1].toInt(), plain.copyOfRange(2, PLAIN_SIZE))
        } finally {
            plain.fill(0)
        }
    }

    private fun write(state: State) {
        require(state.record.size == TAG_RECORD_SIZE) { "Invalid PIN verifier size" }
        val plain = byteArrayOf(VERSION, state.failures.toByte()) + state.record
        val wrapped = try {
            keys.wrap(plain)
        } finally {
            plain.fill(0)
        }
        val temp = File(root, "app-lock.tmp")
        FileOutputStream(temp).use {
            it.write(wrapped)
            it.fd.sync()
        }
        Files.move(temp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE,
            StandardCopyOption.REPLACE_EXISTING)
        FileChannel.open(root.toPath(), StandardOpenOption.READ).use { it.force(true) }
    }
}
