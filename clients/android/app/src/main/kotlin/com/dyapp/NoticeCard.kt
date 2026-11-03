package com.dyapp

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatNotice

private val announced = Modifier.semantics { liveRegion = LiveRegionMode.Polite }

// What the card shows for one identity.
private class NoticeState(allowed: Boolean) {
    // Null while loading or when the store does not open: the switch stays disabled.
    var on by mutableStateOf<Boolean?>(null)
    var failed by mutableStateOf(false)
    var allowed by mutableStateOf(allowed)

    // Android 13+ shows its permission dialog once here; after a refusal Allow opens the settings.
    var asked = false
}

private fun permissionDialogFits(context: Context, asked: Boolean) =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !asked &&
        context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED

// True when on, null when the store does not open.
private suspend fun loadNotice(active: Opened.Ready) = withContext(Dispatchers.IO) {
    runCatching { RustBridge.withChats(active) { it.notice() != ChatNotice.OFF } }.getOrNull()
}

// On stores the fixed text; false when the store refused.
private suspend fun saveNotice(active: Opened.Ready, on: Boolean) = withContext(Dispatchers.IO) {
    val notice = if (on) ChatNotice.FIXED else ChatNotice.OFF
    runCatching { RustBridge.withChats(active) { it.setNotice(notice) } }.isSuccess
}

// Asks for notices: the permission dialog while it fits, otherwise the app's notification settings.
@Composable
private fun rememberAllow(state: NoticeState): () -> Unit {
    val context = LocalContext.current
    val settings = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        state.allowed = noticesAllowed(context)
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        state.asked = true
        state.allowed = noticesAllowed(context)
    }
    return {
        if (permissionDialogFits(context, state.asked)) {
            permission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            settings.launch(
                Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                    .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
            )
        }
    }
}

/**
 * The Settings card that turns the new-mail notice on or off for the active identity. Turning it on
 * asks for the notification permission on Android 13+; while Android blocks notices the card says
 * so and Allow asks again or, once asked, opens the app's notification settings.
 */
@Composable
internal fun NoticeCard(active: Opened.Ready) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val state = remember(active.slot.id) { NoticeState(noticesAllowed(context)) }
    val allow = rememberAllow(state)
    LaunchedEffect(active.slot.id) {
        state.on = loadNotice(active)
        state.failed = state.on == null
    }
    NoticeCardContent(state, allow) { turnOn ->
        scope.launch {
            val saved = saveNotice(active, turnOn)
            state.failed = !saved
            if (saved) state.on = turnOn
            if (saved && turnOn && !state.allowed) allow()
        }
    }
}

@Composable
private fun NoticeCardContent(state: NoticeState, allow: () -> Unit, toggle: (Boolean) -> Unit) =
    FormCard(Modifier.testTag("privacy")) {
        CardTitle(R.string.settings_notices)
        val checked = state.on == true
        Row(
            Modifier.fillMaxWidth().testTag("message-notifications")
                .toggleable(checked, enabled = state.on != null, role = Role.Switch, onValueChange = toggle),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.notice_toggle))
                Caption(stringResource(R.string.notice_toggle_hint))
            }
            Switch(checked, onCheckedChange = null)
        }
        if (state.failed) {
            Caption(stringResource(R.string.notice_failed), Modifier.testTag("notice-error").then(announced))
        }
        if (checked && !state.allowed) {
            Caption(stringResource(R.string.notice_blocked), Modifier.testTag("notice-blocked").then(announced))
            TextButton(onClick = allow, Modifier.testTag("notice-allow")) {
                Text(stringResource(R.string.notice_allow))
            }
        }
    }
