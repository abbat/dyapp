package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

@Composable
internal fun DeviceDirectoryMenu(opened: Opened.Ready, route: MutableState<SettingsPane>) {
    val count by rememberNativeRead(opened) {
        val calls = nativeDeviceDirectoryCalls(opened)
        try {
            calls.read(false).devices.size
        } finally {
            calls.close()
        }
    }
    val open = { route.value = SettingsPane.DEVICES }
    val label = count?.getOrNull()?.let { stringResource(R.string.devices_count, it) }
        ?: stringResource(if (count == null) R.string.devices_count_loading else R.string.devices_count_unknown)
    TextButton(open, Modifier.fillMaxWidth().testTag("settings-devices-open").applicationClick(open)) { Text(label) }
}

/** Group display characters only; the dedicated copy action always receives the unchanged raw key. */
@Composable
internal fun DeviceIdentifier(
    raw: String,
    tag: String,
    label: Int = R.string.devices_key,
    copyTag: String = "copy-$tag"
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Caption(stringResource(label))
        IdentifierText(raw, Modifier.testTag(tag))
        CopyButton(raw, copyTag)
    }
}
