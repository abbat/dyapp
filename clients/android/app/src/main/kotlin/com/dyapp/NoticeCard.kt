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
import androidx.compose.foundation.selection.selectableGroup
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

// The content choices in order, with their test tags and labels.
private val contents = listOf(
    Triple(ChatNotice.FIXED, "notice-fixed", R.string.notice_content_fixed),
    Triple(ChatNotice.NAME, "notice-name", R.string.notice_content_name),
    Triple(ChatNotice.TEXT, "notice-text", R.string.notice_content_text),
)

// What the card shows for one identity.
private class NoticeState(allowed: Boolean) {
    // Null while loading or when the store does not open: the controls stay disabled.
    var on by mutableStateOf<Boolean?>(null)

    // Kept while off, so a turn-on brings the earlier choice back.
    var content by mutableStateOf(ChatNotice.FIXED)
    var failed by mutableStateOf(false)
    var allowed by mutableStateOf(allowed)

    // Android 13+ shows its permission dialog once here; after a refusal Allow opens the settings.
    var asked = false
}

private fun permissionDialogFits(context: Context, asked: Boolean) =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !asked &&
        context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED

// The switch and the content, or null when the store does not open.
private suspend fun loadNotice(active: Opened.Ready) = withContext(Dispatchers.IO) {
    runCatching {
        RustBridge.withChats(active) { (it.notice() != ChatNotice.OFF) to it.noticeContent() }
    }.getOrNull()
}

// False when the store refused.
private suspend fun saveNotice(active: Opened.Ready, notice: ChatNotice) = withContext(Dispatchers.IO) {
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
 * The Settings privacy card: the read receipt switch ([ReadReceiptsRow]), then the new-mail notice.
 * It turns the notice on or off for the active identity and picks
 * what it shows. Turning it on asks for the notification permission on Android 13+; while Android
 * blocks notices the card says so and Allow asks again or, once asked, opens the app's notification
 * settings.
 */
@Composable
internal fun NoticeCard(active: Opened.Ready) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val state = remember(active.slot.id) { NoticeState(noticesAllowed(context)) }
    val allow = rememberAllow(state)
    LaunchedEffect(active.slot.id) {
        val loaded = loadNotice(active)
        state.failed = loaded == null
        loaded?.let { (on, content) ->
            state.content = content
            state.on = on
        }
    }
    NoticeCardContent(state, allow, { ReadReceiptsRow(active) }) { on, content ->
        scope.launch {
            val saved = saveNotice(active, if (on) content else ChatNotice.OFF)
            state.failed = !saved
            if (saved) {
                state.on = on
                state.content = content
            }
            if (saved && on && !state.allowed) allow()
        }
    }
}

@Composable
private fun NoticeContentChoice(chosen: ChatNotice, enabled: Boolean, pick: (ChatNotice) -> Unit) =
    Column(Modifier.selectableGroup().testTag("notice-content")) {
        Text(stringResource(R.string.notice_content))
        contents.forEach { (content, tag, label) ->
            ChoiceRow(tag, label, content == chosen, enabled) { pick(content) }
        }
        Caption(stringResource(R.string.notice_content_hint))
    }

@Composable
private fun NoticeCardContent(
    state: NoticeState,
    allow: () -> Unit,
    receipts: @Composable () -> Unit,
    save: (Boolean, ChatNotice) -> Unit,
) =
    FormCard(Modifier.testTag("privacy")) {
        CardTitle(R.string.settings_notices)
        receipts()
        val checked = state.on == true
        Row(
            Modifier.fillMaxWidth().testTag("message-notifications").toggleable(
                checked, enabled = state.on != null, role = Role.Switch,
                onValueChange = { save(it, state.content) }
            ),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.notice_toggle), Modifier.weight(1f))
            Switch(checked, onCheckedChange = null)
        }
        NoticeContentChoice(state.content, enabled = checked) { save(true, it) }
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
