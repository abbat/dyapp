package com.dyapp

import uniffi.dyapp.KeyStore
import uniffi.dyapp.MobileAppLock
import uniffi.dyapp.MobileAppLockFormat
import uniffi.dyapp.MobileAppLockMode
import uniffi.dyapp.MobilePinAttempt
import java.io.File

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
 * Android ownership/presentation facade for the shared app-wide core lock. Calls block and belong
 * off the UI thread. The UI must freshly authenticate with the OS before recovery or a protected
 * biometric switch; record framing, counters and transitions are decided by the core.
 */
class AppLockStore(root: File, keys: KeyStore) {
    private val files = AppLockFiles(root, keys)

    /** Read the core choice; invalid or unavailable storage must keep the app locked. */
    fun mode(): StoredLock = withCore { it.mode().local() }

    /** Initial setup is atomic; protected replacement requires fresh OS authentication. */
    fun useBiometric(afterDeviceCredential: Boolean = false) = withCore {
        if (afterDeviceCredential) it.useBiometric() else it.useBiometricInitial()
    }

    /** Store Skip and return the actual core mode without disabling an existing lock. */
    fun skip(): StoredLock = withCore {
        it.skip()
        it.mode().local()
    }

    /** Enroll only from New/Off; authenticated protected replacement uses recovery instead. */
    fun enroll(pinValue: String) = withCore { it.enrollInitial(pinValue) }

    /** Core persists the consumed attempt before verification and the reset before success. */
    fun attempt(pinValue: String): PinAttempt = withCore { it.attempt(pinValue).local() }

    /** Authenticate with the current PIN before replacing it or disabling the lock. */
    fun change(current: String, newPin: String?): PinAttempt = withCore { it.change(current, newPin).local() }

    /** Invoke only after a fresh OS device-credential or biometric check. */
    fun resetAfterDeviceCredential(newPin: String?) = withCore { it.resetAfterDeviceCredential(newPin) }

    private fun <T> withCore(action: (MobileAppLock) -> T): T = synchronized(ioLock) {
        MobileAppLock(MobileAppLockFormat.ANDROID, files).use(action)
    }

    /** App-wide storage names remain compatible with existing installations. */
    companion object {
        /** Keystore alias shared by all identity slots for the app lock. */
        const val ALIAS = "dyapp-app-lock"

        /** Marker of the first-launch Skip choice; it holds no data. */
        const val SKIPPED = "app-lock-skipped"

        /** Marker of biometric mode; it holds no data. */
        const val BIOMETRIC = "app-lock-biometric"

        private val ioLock = Any()

        /** Use one app-wide Keystore key, never an identity slot alias. */
        fun onDevice(root: File) = AppLockStore(root, PlatformKeyStore(ALIAS))
    }
}

private fun MobileAppLockMode.local(): StoredLock = when (this) {
    MobileAppLockMode.NEW -> StoredLock.NEW
    MobileAppLockMode.OFF -> StoredLock.OFF
    MobileAppLockMode.PIN -> StoredLock.PIN
    MobileAppLockMode.BIOMETRIC -> StoredLock.BIOMETRIC
}

private fun MobilePinAttempt.local(): PinAttempt = when (this) {
    MobilePinAttempt.UNLOCKED -> PinAttempt.UNLOCKED
    MobilePinAttempt.WRONG -> PinAttempt.WRONG
    MobilePinAttempt.DEVICE_CREDENTIAL_REQUIRED -> PinAttempt.DEVICE_CREDENTIAL_REQUIRED
}
