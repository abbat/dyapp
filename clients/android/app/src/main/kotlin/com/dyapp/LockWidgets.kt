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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp

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

/** The OS screen lock is the recovery method; without one, say how to restore it. */
@Composable
internal fun DeviceCredentialButton(deviceSecure: Boolean, onClick: () -> Unit) {
    OutlinedButton(onClick = onClick, Modifier.testTag("pin-device-unlock"), enabled = deviceSecure) {
        Text(stringResource(R.string.lock_use_device_credential))
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
