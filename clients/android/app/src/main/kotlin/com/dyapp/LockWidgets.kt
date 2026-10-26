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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

internal const val PIN_MIN = 6
private const val PIN_MAX = 64

@Composable
internal fun LockColumn(content: @Composable ColumnScope.() -> Unit) = Column(
    modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
    horizontalAlignment = Alignment.CenterHorizontally,
    content = content
)

@Composable
internal fun PinField(state: MutableState<String>, label: Int, tag: String, enabled: Boolean) {
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
internal fun ErrorText(id: Int) = Text(stringResource(id), Modifier.testTag("pin-error"))

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
 * @property onBiometric chooses biometrics in place of a PIN.
 */
class DeviceLock(
    val secure: Boolean,
    val biometrics: Biometrics = Biometrics.ABSENT,
    val onBiometric: suspend () -> Unit = {},
)

/** Offers device biometrics in place of a PIN, or says in place why they are unavailable. */
@Composable
internal fun BiometricOption(device: DeviceLock, enabled: Boolean) {
    val scope = rememberCoroutineScope()
    when (device.biometrics) {
        Biometrics.READY -> OutlinedButton(
            onClick = { scope.launch { device.onBiometric() } },
            Modifier.testTag("lock-biometric"),
            enabled = enabled && device.secure
        ) { Text(stringResource(R.string.lock_use_biometric)) }
        Biometrics.NOT_ENROLLED ->
            Text(stringResource(R.string.lock_biometric_none), Modifier.testTag("lock-biometric-unavailable"))
        Biometrics.ABSENT -> Unit
    }
}

/** The OS screen lock is the recovery method; without one, say how to restore it. */
@Composable
internal fun DeviceCredentialButton(deviceSecure: Boolean, label: Int, onClick: () -> Unit) {
    OutlinedButton(onClick = onClick, Modifier.testTag("pin-device-unlock"), enabled = deviceSecure) {
        Text(stringResource(label))
    }
    if (!deviceSecure) {
        Text(stringResource(R.string.lock_recovery_unavailable), Modifier.testTag("lock-recovery-unavailable"))
    }
}

@Composable
internal fun NewPinFields(pin: MutableState<String>, repeat: MutableState<String>, label: Int, enabled: Boolean) {
    PinField(pin, label, "pin-new", enabled)
    PinField(repeat, R.string.lock_pin_repeat, "pin-repeat", enabled)
}

@Composable
internal fun SetPinButton(label: Int, onSet: (() -> Unit)?) {
    // A null action shows the button disabled.
    Button(onClick = { onSet?.invoke() }, Modifier.testTag("pin-set"), enabled = onSet != null) {
        Text(stringResource(label))
    }
}
