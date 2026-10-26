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
    fun hash(pin: String): ByteArray
    fun verify(pin: String, record: ByteArray): Boolean
}

/** Shared Argon2id implementation from the Rust UniFFI library. */
object RustAppPinVerifier : AppPinVerifier {
    override fun hash(pin: String): ByteArray = hashAppPin(pin)
    override fun verify(pin: String, record: ByteArray): Boolean = verifyAppPin(pin, record)
}

enum class PinAttempt {
    UNLOCKED, WRONG, DEVICE_CREDENTIAL_REQUIRED
}

/**
 * App-wide protected PIN record and failure counter, independent of identity slots. The UI must
 * authenticate with the OS before calling [resetAfterDeviceCredential]. Every attempted PIN is
 * persisted before KDF work, so a process crash cannot erase a failed attempt.
 */
class AppLockStore(private val root: File, private val keys: KeyStore, private val pin: AppPinVerifier) {
    companion object {
        const val ALIAS = "dyapp-app-lock"

        /** Marker of the first-launch Skip choice; it holds no data. */
        const val SKIPPED = "app-lock-skipped"

        /** Use one app-wide Keystore key, never an identity slot alias. */
        fun onDevice(root: File) = AppLockStore(root, PlatformKeyStore(ALIAS), RustAppPinVerifier)

        private const val VERSION: Byte = 1
        private const val TAG_RECORD_SIZE = 49
        private const val PLAIN_SIZE = 2 + TAG_RECORD_SIZE
        private const val MAX_FAILURES = 5
        private val ioLock = Any()
    }

    private val file = File(root, "app-lock")
    private val skipped = File(root, SKIPPED)

    private data class State(val failures: Int, val record: ByteArray)

    /** Whether an app PIN has been enrolled; a corrupt record throws and must keep the app locked. */
    fun hasPin(): Boolean = synchronized(ioLock) { read() != null }

    /** Whether first launch already enrolled a PIN or chose Skip; a corrupt record throws. */
    fun setupDone(): Boolean = synchronized(ioLock) { read() != null || skipped.exists() }

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

    /** Call only after a fresh OS device-credential check; null PIN clears the app PIN. */
    fun resetAfterDeviceCredential(newPin: String?) = synchronized(ioLock) {
        check(read() != null) { "App PIN is not enrolled" }
        if (newPin == null) {
            check(file.delete()) { "Cannot clear app PIN" }
            skipped.createNewFile() // Clearing is a choice too; first-launch setup stays done.
        } else {
            write(State(0, pin.hash(newPin)))
        }
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
