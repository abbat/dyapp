package com.dyapp

import android.app.Activity
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricManager.Authenticators
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.view.WindowManager
import uniffi.dyapp.MobileAppLockEvent
import uniffi.dyapp.MobileAppLockTimer

/**
 * Ask the OS for the device screen lock, or with [biometric] for strong biometrics first
 * (framework API 30+, no dependency). Cancel and errors keep the app locked; [onSuccess] runs on
 * the main thread.
 */
internal fun Activity.confirmDeviceCredential(biometric: Boolean, onSuccess: () -> Unit) {
    val credential = Authenticators.DEVICE_CREDENTIAL
    val allowed = if (biometric) Authenticators.BIOMETRIC_STRONG or credential else credential
    BiometricPrompt.Builder(this)
        .setTitle(getString(R.string.lock_device_credential_title))
        .setAllowedAuthenticators(allowed)
        .build()
        .authenticate(
            CancellationSignal(),
            mainExecutor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    if (!isDestroyed) onSuccess()
                }
            }
        )
}

/** Whether strong biometrics can be offered; only enrolled ones unlock the app. */
internal fun Context.biometrics() =
    when (getSystemService(BiometricManager::class.java).canAuthenticate(Authenticators.BIOMETRIC_STRONG)) {
        BiometricManager.BIOMETRIC_SUCCESS -> Biometrics.READY
        BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> Biometrics.NOT_ENROLLED
        else -> Biometrics.ABSENT
    }

/**
 * Keeps private content out of the app-switcher snapshot while a PIN is set. Before API 33 only
 * FLAG_SECURE does that, and it also blocks screenshots.
 */
internal fun Activity.obscureSnapshot(hidden: Boolean) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        setRecentsScreenshotEnabled(!hidden)
    } else if (hidden) {
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    } else {
        window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }
}

/**
 * App-wide lock timing, driven by a monotonic millisecond clock such as elapsedRealtime.
 * The core observes expiration on every event/read; UI schedules the next [millisUntilLock] deadline.
 */
class AppLockLifecycle(pinEnabled: Boolean, private val now: () -> Long) : AutoCloseable {
    // elapsedRealtime is nonnegative; reject invalid injected clocks before unsigned lifting.
    private val time get() = now().also { check(it >= 0) }.toULong()
    private val core = MobileAppLockTimer(pinEnabled, time)
    private val snapshot get() = core.snapshot(time)

    /** Whether a PIN protects the current app session. */
    val isPinEnabled: Boolean get() = snapshot.enabled

    /** Current lock state after applying any elapsed deadline. */
    val isLocked: Boolean get() = snapshot.locked

    /** Whether the app-switcher snapshot must hide private content. */
    val snapshotObscured: Boolean get() = snapshot.obscured

    /** Forward an actual platform background transition. */
    fun onBackground() = transition(MobileAppLockEvent.BACKGROUND)

    /** Forward an actual platform foreground transition. */
    fun onForeground() = transition(MobileAppLockEvent.FOREGROUND)

    /** Forward actual user interaction; core checks expiration first. */
    fun onInteraction() = transition(MobileAppLockEvent.INTERACTION)

    /** Call after successful foreground PIN or OS authentication. */
    fun onUnlocked() = transition(MobileAppLockEvent.UNLOCKED)

    /** Call after successful enrollment or authorized protected-mode change. */
    fun onPinEnrolled() = transition(MobileAppLockEvent.ENROLLED)

    /** Call only after an authorized setting change. */
    fun onPinCleared() = transition(MobileAppLockEvent.CLEARED)

    /** Core-derived scheduling delay; all valid delays fit in platform milliseconds. */
    fun millisUntilLock(): Long? = snapshot.remainingMs?.toLong()

    /** Release this Activity-owned native timer. */
    override fun close() = core.close()

    private fun transition(event: MobileAppLockEvent) {
        core.transition(event, time)
    }
}
