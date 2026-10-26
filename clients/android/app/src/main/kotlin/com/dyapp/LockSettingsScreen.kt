package com.dyapp

import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
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

/**
 * App-wide lock settings. With a PIN set, Change and Turn off both need the current PIN, checked
 * as one counted unlock attempt. [onSetPin] takes the current PIN (null when none is set) and the
 * new one (null turns the PIN off), runs off the UI thread and returns null on a storage error.
 */
@Composable
fun LockSettingsScreen(pinSet: Boolean, deviceSecure: Boolean, onSetPin: PinSetter, onBack: () -> Unit) {
    val scope = rememberCoroutineScope()
    val form = remember { PinForm() }
    val open = form.open
    val authorized = form.authorized(pinSet)
    val submit = { newPin: String? -> form.submit(scope, form.current.value.takeIf { pinSet }, newPin, onSetPin) }
    LockColumn {
        LockSettingsStatus(pinSet, deviceSecure, form.gated)
        if (pinSet) PinField(form.current, R.string.lock_pin_current, "pin-current", open)
        NewPinFields(form.pin, form.repeat, R.string.lock_pin_new, open && deviceSecure)
        form.error?.let { ErrorText(it) }
        LockSettingsActions(
            pinSet,
            { submit(form.pin.value) }.takeIf { authorized && deviceSecure && form.pin.value.isNotEmpty() },
            { submit(null) }.takeIf { authorized && pinSet },
            onBack.takeIf { !form.busy }
        )
    }
}

@Composable
private fun LockSettingsStatus(pinSet: Boolean, deviceSecure: Boolean, gated: Boolean) {
    val mode = if (pinSet) R.string.lock_mode_pin else R.string.lock_mode_off
    Text(stringResource(mode), Modifier.testTag("lock-mode"))
    if (!deviceSecure) NoScreenLockText()
    // The OS credential check that resets the count is planned.
    if (gated) DeviceCredentialText()
}

@Composable
private fun LockSettingsActions(pinSet: Boolean, onSet: (() -> Unit)?, onOff: (() -> Unit)?, onBack: (() -> Unit)?) {
    // A null action shows its button disabled.
    SetPinButton(if (pinSet) R.string.lock_change else R.string.lock_set, onSet)
    if (pinSet) {
        OutlinedButton(onClick = { onOff?.invoke() }, Modifier.testTag("lock-off"), enabled = onOff != null) {
            Text(stringResource(R.string.lock_turn_off))
        }
    }
    OutlinedButton(onClick = { onBack?.invoke() }, Modifier.testTag("lock-back"), enabled = onBack != null) {
        Text(stringResource(R.string.lock_back))
    }
}
