package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

private const val PIN_MIN = 6
private const val PIN_MAX = 64

@Composable
private fun LockColumn(content: @Composable ColumnScope.() -> Unit) = Column(
    modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
    horizontalAlignment = Alignment.CenterHorizontally,
    content = content
)

@Composable
private fun PinField(state: MutableState<String>, label: Int, tag: String, enabled: Boolean) {
    // Keep only the ASCII digits the Rust verifier accepts.
    OutlinedTextField(
        value = state.value,
        onValueChange = { input -> state.value = input.filter { it in '0'..'9' }.take(PIN_MAX) },
        modifier = Modifier.testTag(tag),
        enabled = enabled,
        label = { Text(stringResource(label)) },
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
        singleLine = true
    )
}

@Composable
private fun ErrorText(id: Int) = Text(stringResource(id), Modifier.testTag("pin-error"))

private fun pinRejection(pin: String, repeat: String) = when {
    pin.length < PIN_MIN -> R.string.lock_pin_short
    pin != repeat -> R.string.lock_pin_mismatch
    else -> null
}

/**
 * First launch: Skip, or enrol a PIN. A PIN needs a device screen lock, its recovery method,
 * so without one the option is explained in place. [onEnroll] runs the KDF off the UI thread.
 */
@Composable
fun SetupScreen(deviceSecure: Boolean, onSkip: () -> Unit, onEnroll: suspend (String) -> Boolean) {
    val scope = rememberCoroutineScope()
    val pinState = remember { mutableStateOf("") }
    val repeatState = remember { mutableStateOf("") }
    var pin by pinState
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val editable = deviceSecure && !busy
    LockColumn {
        SetupIntro(deviceSecure)
        PinField(pinState, R.string.lock_pin, "pin-new", editable)
        PinField(repeatState, R.string.lock_pin_repeat, "pin-repeat", editable)
        error?.let { ErrorText(it) }
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
        SetupActions(submit.takeIf { editable && pin.isNotEmpty() }, onSkip.takeIf { !busy })
    }
}

@Composable
private fun SetupIntro(deviceSecure: Boolean) {
    Text(stringResource(R.string.lock_setup_title), Modifier.testTag("lock-setup"))
    Text(stringResource(R.string.lock_setup_body))
    if (!deviceSecure) Text(stringResource(R.string.lock_no_screen_lock), Modifier.testTag("lock-unavailable"))
}

@Composable
private fun SetupActions(onSet: (() -> Unit)?, onSkip: (() -> Unit)?) {
    // A null action shows its button disabled.
    Button(onClick = { onSet?.invoke() }, Modifier.testTag("pin-set"), enabled = onSet != null) {
        Text(stringResource(R.string.lock_set))
    }
    OutlinedButton(onClick = { onSkip?.invoke() }, Modifier.testTag("lock-skip"), enabled = onSkip != null) {
        Text(stringResource(R.string.lock_skip))
    }
}

/**
 * The only surface while locked: no profile, message or Peer ID is composed. [onAttempt] runs
 * the KDF off the UI thread and returns null when the record cannot be checked.
 */
@Composable
fun UnlockScreen(onAttempt: suspend (String) -> PinAttempt?) {
    val scope = rememberCoroutineScope()
    val pinState = remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var gated by remember { mutableStateOf(false) }
    LockColumn {
        Text(stringResource(R.string.app_locked), Modifier.testTag("app-locked"))
        // The OS credential check that resets the count is planned; until then stay here.
        if (gated) {
            Text(stringResource(R.string.lock_device_credential), Modifier.testTag("pin-device-credential"))
            return@LockColumn
        }
        PinField(pinState, R.string.lock_pin, "pin", !busy)
        error?.let { ErrorText(it) }
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
        Button(onClick = submit, Modifier.testTag("pin-unlock"), enabled = !busy && pinState.value.length >= PIN_MIN) {
            Text(stringResource(R.string.lock_unlock))
        }
    }
}

private fun attemptError(result: PinAttempt?) = when (result) {
    PinAttempt.UNLOCKED, PinAttempt.DEVICE_CREDENTIAL_REQUIRED -> null
    PinAttempt.WRONG -> R.string.lock_pin_wrong
    null -> R.string.lock_failed
}
