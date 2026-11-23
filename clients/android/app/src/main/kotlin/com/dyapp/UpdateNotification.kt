package com.dyapp

import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.util.Log
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdatePlatform

internal const val UPDATE_NOTICE_ID = 2
internal const val UPDATE_NOTICE_CHANNEL = "updates"

/** Platform admission only; the shared core selects and durably claims the cached offer. */
internal fun deliverCachedUpdate(
    allowed: () -> Boolean,
    claim: () -> MobileUpdateOffer?,
    post: (MobileUpdateOffer) -> Unit
) {
    if (!allowed()) return
    val offer = claim() ?: return
    if (allowed()) post(offer)
}

/** No identity lease, HTTP, installation or background permission prompt. */
internal fun postCachedUpdate(context: Context, active: () -> Boolean) {
    runCatching {
        deliverCachedUpdate(
            allowed = { active() && updateNoticesAllowed(context) },
            claim = {
                MobileClientUpdates(context.noBackupFilesDir.absolutePath, MobileUpdatePlatform.ANDROID).use {
                    it.claimPendingNotification()
                }
            },
            post = { offer ->
                val notice = updateNotification(context, offer)
                if (active() && updateNoticesAllowed(context)) {
                    context.getSystemService(NotificationManager::class.java).notify(UPDATE_NOTICE_ID, notice)
                }
            }
        )
    }.onFailure { Log.i("dyapp", "Cached update notice failed: ${it.javaClass.simpleName}") }
}

private fun updateNoticesAllowed(context: Context): Boolean {
    val process = ActivityManager.RunningAppProcessInfo().also { ActivityManager.getMyMemoryState(it) }
    if (process.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND || !noticesAllowed(context)) {
        return false
    }
    val channel = updateNotificationChannel(context).getNotificationChannel(UPDATE_NOTICE_CHANNEL)
    return channel != null && channel.importance != NotificationManager.IMPORTANCE_NONE
}

private fun updateNotificationChannel(context: Context): NotificationManager {
    val manager = context.getSystemService(NotificationManager::class.java)
    manager.createNotificationChannel(
        NotificationChannel(
            UPDATE_NOTICE_CHANNEL,
            context.getString(R.string.settings_updates),
            NotificationManager.IMPORTANCE_DEFAULT
        )
    )
    return manager
}

internal fun updateNotificationIntent(offer: MobileUpdateOffer): Intent =
    Intent(Intent.ACTION_VIEW, Uri.parse(offer.releasePage)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

/** The immutable tap opens exactly the core-provided official information page, never an installer. */
internal fun updateNotification(context: Context, offer: MobileUpdateOffer): Notification {
    updateNotificationChannel(context)
    val open = updateNotificationIntent(offer)
    val tap = PendingIntent.getActivity(
        context,
        UPDATE_NOTICE_ID,
        open,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val public = Notification.Builder(context, UPDATE_NOTICE_CHANNEL)
        .setSmallIcon(R.drawable.ic_launcher_monochrome)
        .setContentTitle(context.getString(R.string.app_name))
        .build()
    return Notification.Builder(context, UPDATE_NOTICE_CHANNEL)
        .setSmallIcon(R.drawable.ic_launcher_monochrome)
        .setContentTitle(context.getString(R.string.app_name))
        .setContentText(context.getString(R.string.update_available_badge, offer.version))
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(public)
        .setContentIntent(tap)
        .setAutoCancel(true)
        .build()
}
