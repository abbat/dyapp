package com.dyapp

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateState

/** Missing channels use Android's default; only the explicit settings action creates the channel. */
internal fun updateNoticePermission(context: Context): Boolean {
    val manager = context.getSystemService(NotificationManager::class.java)
    val channel = manager.getNotificationChannel(UPDATE_NOTICE_CHANNEL)
    return noticesAllowed(context) && channel?.importance != NotificationManager.IMPORTANCE_NONE
}

internal fun updateNoticeSettings(context: Context): Intent {
    val appAllowed = noticesAllowed(context)
    if (appAllowed) updateNotificationChannel(context)
    return Intent(
        if (appAllowed) Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS else Settings.ACTION_APP_NOTIFICATION_SETTINGS
    ).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName).also {
        if (appAllowed) it.putExtra(Settings.EXTRA_CHANNEL_ID, UPDATE_NOTICE_CHANNEL)
    }
}

@Composable
internal fun UpdateNoticePermission(owner: Any, snapshot: MobileUpdateState?) {
    val offer = snapshot?.offer
    if (snapshot?.availability != MobileUpdateAvailability.AVAILABLE ||
        offer == null || offer.delivery == MobileUpdateDelivery.STORE
    ) {
        return
    }
    key(owner, offer.version, offer.releasePage) { UpdatePermissionRow() }
}

@Composable
private fun UpdatePermissionRow() {
    val context = LocalContext.current
    val opening = remember { UpdatePermissionOpening() }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var allowed by remember { mutableStateOf(runCatching { updateNoticePermission(context) }.getOrNull()) }
    var failed by remember { mutableStateOf(false) }
    val refresh = {
        val result = runCatching { updateNoticePermission(context) }
        allowed = result.getOrNull()
        failed = result.isFailure
    }
    val launch = rememberUpdatePermissionLaunch(opening, refresh)
    DisposableEffect(opening) { onDispose { opening.retire() } }
    UpdatePermissionContent(allowed, failed, opening.pending) {
        if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
            if (allowed == null) {
                refresh()
            } else {
                runCatching(launch).onFailure { failed = true }
            }
        }
    }
}

@Composable
private fun rememberUpdatePermissionLaunch(opening: UpdatePermissionOpening, refresh: () -> Unit): () -> Unit {
    val context = LocalContext.current
    val settings = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        opening.complete(refresh)
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        opening.complete(refresh)
    }
    return {
        val request = notificationPermissionFits(context, opening.asked)
        opening.begin(request) {
            if (request) {
                permission.launch(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                settings.launch(updateNoticeSettings(context))
            }
        }
    }
}

@Composable
private fun UpdatePermissionContent(allowed: Boolean?, failed: Boolean, pending: Boolean, action: () -> Unit) {
    val status = when (allowed) {
        true -> R.string.update_notice_allowed
        false -> R.string.update_notice_blocked
        null -> R.string.update_notice_permission_failed
    }
    Caption(stringResource(status), Modifier.testTag("update-notice-permission"))
    if (failed && allowed != null) Caption(stringResource(R.string.update_notice_permission_failed))
    TextButton(action, Modifier.testTag("update-notice-allow"), enabled = !pending) {
        Text(stringResource(if (allowed == null) R.string.update_notice_retry else R.string.update_notice_manage))
    }
}

@androidx.annotation.ChecksSdkIntAtLeast(api = Build.VERSION_CODES.TIRAMISU)
private fun notificationPermissionFits(context: Context, asked: Boolean) =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !asked &&
        context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
