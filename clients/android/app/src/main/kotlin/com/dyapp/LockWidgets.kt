package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.TextObfuscationMode
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedSecureTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import uniffi.dyapp.appPinInput

@Composable
internal fun LockColumn(content: @Composable ColumnScope.() -> Unit) = Column(
    modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
    horizontalAlignment = Alignment.CenterHorizontally,
    content = content
)

@Composable
internal fun PinField(state: MutableState<String>, label: Int, tag: String, enabled: Boolean) {
    val field = remember(state) { TextFieldState(state.value) }
    LaunchedEffect(field) {
        snapshotFlow { field.text.toString() }.collect { input ->
            if (state.value != input) state.value = input
        }
    }
    LaunchedEffect(field) {
        snapshotFlow { state.value }.collect { input ->
            if (field.text.toString() != input) field.edit { replace(0, length, input) }
        }
    }
    // The core supplies the same ASCII/cap assistance as its PIN format.
    OutlinedSecureTextField(
        state = field,
        modifier = Modifier.fillMaxWidth().testTag(tag),
        enabled = enabled,
        label = { Text(stringResource(label)) },
        inputTransformation = {
            val input = toString()
            val filtered = appPinInput(input)
            if (filtered != input) replace(0, length, filtered)
        },
        textObfuscationMode = TextObfuscationMode.Hidden,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword)
    )
}

@Composable
internal fun ErrorText(id: Int) = Text(
    stringResource(id),
    Modifier.testTag("lock-error"),
    color = MaterialTheme.colorScheme.error,
    style = MaterialTheme.typography.bodySmall
)

@Composable
internal fun NoScreenLockText() =
    Text(stringResource(R.string.lock_no_screen_lock), Modifier.testTag("lock-unavailable"))

@Composable
internal fun DeviceCredentialText() =
    Text(stringResource(R.string.lock_device_credential), Modifier.testTag("pin-device-credential"))

/** Whether the device has a usable strong biometric authenticator. */
enum class Biometrics {
    /** No hardware, or the OS does not offer it to apps. */
    ABSENT,

    /** The hardware is there, but no fingerprint or face is enrolled. */
    NOT_ENROLLED,

    /** BiometricPrompt can authenticate with a strong biometric. */
    READY,
}

/**
 * The device unlock methods.
 *
 * @property secure whether a device screen lock is set.
 * @property biometrics whether biometrics can be offered.
 * @property onBiometric returns whether the admitted biometric choice completed.
 */
class DeviceLock(
    val secure: Boolean,
    val biometrics: Biometrics = Biometrics.ABSENT,
    val onBiometric: suspend () -> Boolean = { false },
)

/** Offers device biometrics in place of a PIN, or says in place why they are unavailable. */
@Composable
internal fun BiometricOption(device: DeviceLock, enabled: Boolean, onChoose: () -> Unit) {
    when (device.biometrics) {
        Biometrics.READY -> OutlinedButton(
            onClick = onChoose,
            Modifier.fillMaxWidth().testTag("lock-biometric").applicationClick(onChoose),
            enabled = enabled && device.secure
        ) { Text(stringResource(R.string.lock_use_biometric)) }
        Biometrics.NOT_ENROLLED ->
            Text(stringResource(R.string.lock_biometric_none), Modifier.testTag("lock-biometric-unavailable"))
        Biometrics.ABSENT -> Unit
    }
}

/** The OS screen lock is the recovery method; without one, say how to restore it. */
@Composable
internal fun DeviceCredentialButton(deviceSecure: Boolean, label: Int, primary: Boolean, onClick: () -> Unit) {
    val modifier = Modifier.fillMaxWidth().testTag("lock-device")
    if (primary) {
        Button(onClick = onClick, modifier, enabled = deviceSecure) { Text(stringResource(label)) }
    } else {
        TextButton(onClick = onClick, modifier, enabled = deviceSecure) { Text(stringResource(label)) }
    }
    if (!deviceSecure) {
        Text(stringResource(R.string.lock_recovery_unavailable), Modifier.testTag("lock-recovery-unavailable"))
    }
}

@Composable
internal fun NewPinFields(pin: MutableState<String>, repeat: MutableState<String>, label: Int, enabled: Boolean) {
    PinField(pin, label, "pin-set", enabled)
    PinField(repeat, R.string.lock_pin_repeat, "pin-repeat", enabled)
}

@Composable
internal fun SetPinButton(tag: String, label: Int, onSet: (() -> Unit)?) {
    // A null action shows the button disabled.
    Button(
        onClick = { onSet?.invoke() },
        Modifier.fillMaxWidth().testTag(tag).applicationClick { onSet?.invoke() },
        enabled = onSet != null
    ) {
        Text(stringResource(label))
    }
}
