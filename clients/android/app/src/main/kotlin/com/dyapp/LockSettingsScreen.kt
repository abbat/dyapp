package com.dyapp

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

private class PinForm {
    val current = mutableStateOf("")
    val pin = mutableStateOf("")
    val repeat = mutableStateOf("")
    var busy by mutableStateOf(false)
    var error by mutableStateOf<Int?>(null)
    var gated by mutableStateOf(false)
    val open get() = !busy && !gated

    fun authorized(pinSet: Boolean) = open && (!pinSet || current.value.length >= PIN_MIN)

    fun submit(scope: CoroutineScope, current: String?, newPin: String?, onSetPin: PinSetter) {
        // Every submit clears the typed PINs, accepted or not.
        error = newPin?.let { pinRejection(it, repeat.value) }
        listOf(this.current, pin, repeat).forEach { it.value = "" }
        if (error != null) return
        busy = true
        scope.launch {
            val result = onSetPin(current, newPin)
            gated = result == PinAttempt.DEVICE_CREDENTIAL_REQUIRED
            error = attemptError(result)
            busy = false
        }
    }
}

private typealias PinSetter = suspend (String?, String?) -> PinAttempt?

/** What the settings screen may change and what it must check first. */
enum class LockMode {
    /** No PIN is set. */
    OFF,

    /** A PIN is set; changes need the current PIN. */
    PIN,

    /** A PIN is set and a fresh OS device-credential check stands in for it. */
    RESET,

    /** Device biometrics replace the PIN; the unlock that opened the app was a fresh check. */
    BIOMETRIC,
}

/**
 * App-wide lock settings. With a PIN set, Change and Turn off both need the current PIN, checked
 * as one counted unlock attempt. [onSetPin] takes the current PIN (null when none is set) and the
 * new one (null turns the lock off), runs off the UI thread and returns null on a storage error.
 * After a fresh OS device-credential check, [LockMode.RESET] replaces or clears it without one.
 * Device biometrics are offered while no PIN guards the choice.
 */
@Composable
fun LockSettingsScreen(mode: LockMode, device: DeviceLock, onSetPin: PinSetter, onBack: () -> Unit) {
    val scope = rememberCoroutineScope()
    val form = remember { PinForm() }
    val open = form.open
    val deviceSecure = device.secure
    val pinSet = mode != LockMode.OFF
    val askCurrent = mode == LockMode.PIN
    val authorized = form.authorized(askCurrent)
    val submit = { newPin: String? -> form.submit(scope, form.current.value.takeIf { askCurrent }, newPin, onSetPin) }
    BackHandler { if (!form.busy) onBack() }
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        Arrangement.spacedBy(12.dp)
    ) {
        LockBackBar(onBack.takeIf { !form.busy })
        LockSettingsStatus(mode, deviceSecure, form.gated)
        if (askCurrent) PinField(form.current, R.string.lock_pin_current, "pin-current", open)
        NewPinFields(form.pin, form.repeat, R.string.lock_pin_new, open && deviceSecure)
        form.error?.let { ErrorText(it) }
        LockSettingsActions(
            mode,
            { submit(form.pin.value) }.takeIf { authorized && deviceSecure && form.pin.value.isNotEmpty() },
            { submit(null) }.takeIf { authorized && pinSet }
        )
        if (mode == LockMode.OFF || mode == LockMode.RESET) BiometricOption(device, open)
    }
}

internal fun lockModeText(mode: LockMode) = when (mode) {
    LockMode.OFF -> R.string.lock_mode_off
    LockMode.BIOMETRIC -> R.string.lock_mode_biometric
    else -> R.string.lock_mode_pin
}

@Composable
private fun LockBackBar(onBack: (() -> Unit)?) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = { onBack?.invoke() }, Modifier.testTag("lock-back"), enabled = onBack != null) {
            Icon(painterResource(R.drawable.ic_back), stringResource(R.string.lock_back))
        }
        Text(
            stringResource(R.string.lock_settings), Modifier.semantics { heading() },
            style = MaterialTheme.typography.titleLarge
        )
    }
}

@Composable
private fun LockSettingsStatus(mode: LockMode, deviceSecure: Boolean, gated: Boolean) {
    Text(
        stringResource(lockModeText(mode)), Modifier.testTag("lock-mode"),
        color = MaterialTheme.colorScheme.onSurfaceVariant
    )
    if (mode != LockMode.BIOMETRIC && deviceSecure) Caption(stringResource(R.string.lock_recovery))
    if (mode == LockMode.RESET) Text(stringResource(R.string.lock_reset), Modifier.testTag("lock-reset"))
    if (!deviceSecure) NoScreenLockText()
    // Only the unlock screen offers the OS check; the app relocks within the idle limit.
    if (gated) DeviceCredentialText()
}

@Composable
private fun LockSettingsActions(mode: LockMode, onSet: (() -> Unit)?, onOff: (() -> Unit)?) {
    // A null action shows its button disabled.
    val pinSet = mode == LockMode.PIN || mode == LockMode.RESET
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp), Alignment.CenterVertically) {
        if (mode != LockMode.OFF) {
            val off = if (pinSet) R.string.lock_turn_off else R.string.lock_turn_off_biometric
            TextButton(
                onClick = { onOff?.invoke() },
                Modifier.weight(1f).testTag("lock-turn-off"),
                enabled = onOff != null,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)
            ) { Text(stringResource(off)) }
        }
        Box(Modifier.weight(1f)) {
            if (pinSet) {
                SetPinButton("lock-change", R.string.lock_change, onSet)
            } else {
                SetPinButton("lock-set", R.string.lock_set, onSet)
            }
        }
    }
}
