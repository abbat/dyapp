package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.launch

internal fun pinRejection(pin: String, repeat: String) = when {
    pin.length < PIN_MIN -> R.string.lock_pin_short
    pin != repeat -> R.string.lock_pin_mismatch
    else -> null
}

/**
 * First launch: Skip, enrol a PIN, or use device biometrics. Both need a device screen lock, the
 * recovery method, so without one the options are explained in place. [onEnroll] runs the KDF
 * off the UI thread.
 */
@Composable
fun SetupScreen(device: DeviceLock, onSkip: () -> Unit, onEnroll: suspend (String) -> Boolean) {
    val scope = rememberCoroutineScope()
    val pinState = remember { mutableStateOf("") }
    val repeatState = remember { mutableStateOf("") }
    var pin by pinState
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val editable = device.secure && !busy
    val submit: () -> Unit = {
        val chosen = pin
        error = pinRejection(chosen, repeatState.value)
        pin = ""
        repeatState.value = ""
        if (error == null) {
            busy = true
            scope.launch {
                if (!onEnroll(chosen)) error = R.string.lock_failed
                busy = false
            }
        }
    }
    Page(actions = {
        SetPinButton("lock-set", R.string.lock_set, submit.takeIf { editable && pin.isNotEmpty() })
        BiometricOption(device, !busy)
        TextButton(onClick = onSkip, Modifier.fillMaxWidth().testTag("lock-skip"), enabled = !busy) {
            Text(stringResource(R.string.lock_skip))
        }
    }) {
        SetupIntro(device.secure)
        NewPinFields(pinState, repeatState, R.string.lock_pin, editable)
        error?.let { ErrorText(it) }
    }
}

@Composable
private fun SetupIntro(deviceSecure: Boolean) {
    Icon(painterResource(R.drawable.ic_lock), contentDescription = null)
    PageTitle(R.string.lock_setup_title, Modifier.testTag("lock-setup"))
    Text(stringResource(R.string.lock_setup_body))
    if (deviceSecure) Caption(R.string.lock_recovery) else NoScreenLockText()
}

/**
 * The only surface while locked: no profile, message or Peer ID is composed. [onAttempt] runs
 * the KDF off the UI thread and returns null when the record cannot be checked.
 * [onDeviceCredential] opens the OS screen-lock check, which also recovers a forgotten PIN.
 */
@Composable
fun UnlockScreen(deviceSecure: Boolean, onAttempt: suspend (String) -> PinAttempt?, onDeviceCredential: () -> Unit) {
    val scope = rememberCoroutineScope()
    val pinState = remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var gated by remember { mutableStateOf(false) }
    val submit: () -> Unit = {
        val entered = pinState.value.also { pinState.value = "" }
        busy = true
        scope.launch {
            val result = onAttempt(entered)
            gated = result == PinAttempt.DEVICE_CREDENTIAL_REQUIRED
            error = attemptError(result)
            busy = false
        }
    }
    Page(actions = {
        if (!gated) UnlockButton(submit.takeIf { !busy && pinState.value.length >= PIN_MIN })
        // Once gated, the device screen lock is the only way in.
        DeviceCredentialButton(deviceSecure, R.string.lock_use_device_credential, gated, onDeviceCredential)
    }) {
        PageTitle(R.string.lock_unlock_title, Modifier.testTag(LOCKED_TAG))
        if (gated) {
            DeviceCredentialText()
        } else {
            PinField(pinState, R.string.lock_pin, "pin-unlock", !busy)
            error?.let { ErrorText(it) }
        }
    }
}

private const val LOCKED_TAG = "app-locked"

/** The locked surface in biometric mode: [onUnlock] opens BiometricPrompt with the OS credential fallback. */
@Composable
fun BiometricUnlockScreen(deviceSecure: Boolean, onUnlock: () -> Unit) = Page(actions = {
    DeviceCredentialButton(deviceSecure, R.string.lock_use_biometric, primary = true, onUnlock)
}) {
    PageTitle(R.string.app_locked, Modifier.testTag(LOCKED_TAG))
}

@Composable
private fun UnlockButton(onUnlock: (() -> Unit)?) {
    // A null action shows the button disabled.
    Button(onClick = { onUnlock?.invoke() }, Modifier.fillMaxWidth().testTag("lock-unlock"), enabled = onUnlock != null) {
        Text(stringResource(R.string.lock_unlock))
    }
}

internal fun attemptError(result: PinAttempt?) = when (result) {
    PinAttempt.UNLOCKED, PinAttempt.DEVICE_CREDENTIAL_REQUIRED -> null
    PinAttempt.WRONG -> R.string.lock_pin_wrong
    null -> R.string.lock_failed
}
