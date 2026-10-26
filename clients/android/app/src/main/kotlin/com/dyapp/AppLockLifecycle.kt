package com.dyapp

import android.app.Activity
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricManager.Authenticators
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.view.WindowManager

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
        .authenticate(CancellationSignal(), mainExecutor, object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = onSuccess()
        })
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
 * The UI calls [refresh] at each event and schedules the next [millisUntilLock] deadline.
 */
class AppLockLifecycle(pinEnabled: Boolean, private val now: () -> Long) {
    /** Lock deadlines shared by the activity and its UI timer. */
    companion object {
        /** Time a backgrounded app may remain unlocked. */
        const val BACKGROUND_GRACE_MS = 60_000L
        /** Maximum unlocked foreground idle time. */
        const val FOREGROUND_IDLE_MS = 5 * 60_000L
    }

    private var enabled = pinEnabled
    private var locked = pinEnabled // Every fresh process starts locked when a PIN exists.
    private var backgroundAt: Long? = null
    private var lastInteractionAt = now()

    /** Whether a PIN protects the current app session. */
    val isPinEnabled: Boolean get() = enabled
    /** Current lock state after applying any elapsed deadline. */
    val isLocked: Boolean get() { refresh(); return locked }
    /** Whether the app-switcher snapshot must hide private content. */
    val snapshotObscured: Boolean get() = backgroundAt != null

    /** Call as soon as the activity backgrounds; the snapshot must be covered before the grace ends. */
    fun onBackground() {
        refresh()
        backgroundAt = now()
    }

    /** A quick return preserves unlock; an expired grace period requires a new unlock. */
    fun onForeground() {
        refresh()
        backgroundAt = null
        lastInteractionAt = now()
    }

    /** A late event cannot extend a deadline that has already expired. */
    fun onInteraction() {
        refresh()
        if (backgroundAt == null && !locked) lastInteractionAt = now()
    }

    /** Call after a successful PIN or OS unlock. */
    fun onUnlocked() {
        check(enabled && backgroundAt == null) { "Unlock requires a foreground app PIN" }
        locked = false
        lastInteractionAt = now()
    }

    /** Call after successful enrollment; the user remains in the current unlocked session. */
    fun onPinEnrolled() {
        enabled = true
        locked = false
        lastInteractionAt = now()
    }

    /** Call only after an authorized setting change. */
    fun onPinCleared() {
        enabled = false
        locked = false
    }

    /** Apply elapsed-time deadlines without relying on wall clock or saved activity state. */
    fun refresh() {
        if (!enabled || locked) return
        val start = backgroundAt
        val elapsed = now() - (start ?: lastInteractionAt)
        val limit = if (start == null) FOREGROUND_IDLE_MS else BACKGROUND_GRACE_MS
        if (elapsed >= limit) locked = true
    }

    /** Time until the next lock; null when disabled or already locked. */
    fun millisUntilLock(): Long? {
        refresh()
        if (!enabled || locked) return null
        val start = backgroundAt
        val limit = if (start == null) FOREGROUND_IDLE_MS else BACKGROUND_GRACE_MS
        return (limit - (now() - (start ?: lastInteractionAt))).coerceAtLeast(0)
    }
}
