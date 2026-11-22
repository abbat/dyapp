package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

@Composable
internal fun DeviceDisconnectDialog(
    session: DeviceDirectorySession,
    opening: DeviceDisconnectOpening,
    view: DeviceDirectoryViewState
) {
    AlertDialog(
        onDismissRequest = { session.dismiss(opening) },
        modifier = Modifier.testTag("disconnect-dialog"),
        title = { Text(stringResource(R.string.devices_disconnect_title)) },
        text = { DeviceDisconnectBody(opening, view) },
        confirmButton = { DeviceDisconnectConfirm(session, opening, view) },
        dismissButton = { DeviceDisconnectCancel(session, opening, view.busy) }
    )
}

@Composable
private fun DeviceDisconnectBody(opening: DeviceDisconnectOpening, view: DeviceDirectoryViewState) {
    Column(Modifier.verticalScroll(rememberScrollState()), Arrangement.spacedBy(12.dp)) {
        DeviceIdentifier(opening.target.identifier, "disconnect-device")
        Text(stringResource(R.string.devices_disconnect_text))
        if (view.busy) Text(stringResource(R.string.devices_revoking), Modifier.testTag("disconnect-pending"))
        if (failedDisconnect(opening, view)) {
            Text(stringResource(R.string.devices_uncertain), Modifier.testTag("disconnect-problem"))
        }
    }
}

private fun failedDisconnect(opening: DeviceDisconnectOpening, view: DeviceDirectoryViewState): Boolean =
    view.completion?.let { it.target === opening.target && it.result.isFailure } == true

@Composable
private fun DeviceDisconnectConfirm(
    session: DeviceDirectorySession,
    opening: DeviceDisconnectOpening,
    view: DeviceDirectoryViewState
) {
    val confirm: () -> Unit = { session.confirm(opening) }
    val failed = failedDisconnect(opening, view)
    TextButton(
        confirm,
        Modifier.testTag(if (failed) "disconnect-retry" else "disconnect-confirm").applicationClick(confirm),
        !view.busy
    ) {
        Text(stringResource(if (failed) R.string.devices_retry else R.string.devices_disconnect_action))
    }
}

@Composable
private fun DeviceDisconnectCancel(
    session: DeviceDirectorySession,
    opening: DeviceDisconnectOpening,
    busy: Boolean
) {
    val focus = remember { FocusRequester() }
    val ready = LocalWindowInfo.current.isWindowFocused && !busy
    val cancel: () -> Unit = { session.dismiss(opening) }
    TextButton(
        cancel,
        Modifier.testTag("disconnect-cancel").focusRequester(focus).focusProperties { canFocus = true }
            .applicationClick(cancel),
        !busy
    ) { Text(stringResource(R.string.remove_cancel)) }
    LaunchedEffect(ready) { if (ready) focus.requestFocus() }
}
