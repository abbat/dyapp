package com.dyapp

import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import uniffi.dyapp.ChatMailNotice

private const val MAIL_CHANNEL = "mail"

// One fixed id, so each notice replaces the previous one.
internal const val MAIL_NOTICE_ID = 1

/** Core-admitted sender/body rendered with the existing local name and blank-body fallback. */
internal data class NoticeLines(val sender: String, val text: String?)

/** Null means the core's fixed notice. Kotlin does not select peers, count mail or inspect history. */
internal fun noticeLines(notice: ChatMailNotice): NoticeLines? = notice.peerId?.let { peer ->
    NoticeLines(shortPeerId(peer), notice.text?.ifBlank { null })
}

/**
 * The notice: the fixed "New message" text, or the [lines] the setting allows. The public version
 * for a secure lock screen carries just the app name, whatever the setting.
 */
internal fun mailNotice(context: Context, lines: NoticeLines? = null): Notification {
    val manager = context.getSystemService(NotificationManager::class.java)
    val name = context.getString(R.string.notice_channel)
    manager.createNotificationChannel(NotificationChannel(MAIL_CHANNEL, name, NotificationManager.IMPORTANCE_DEFAULT))
    val open = Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    // The tap only shows the app; the app lock still asks first.
    val tap = PendingIntent.getActivity(context, 0, open, PendingIntent.FLAG_IMMUTABLE)
    val public = Notification.Builder(context, MAIL_CHANNEL)
        .setSmallIcon(R.drawable.ic_launcher_monochrome)
        .setContentTitle(context.getString(R.string.app_name))
        .build()
    return Notification.Builder(context, MAIL_CHANNEL)
        .setSmallIcon(R.drawable.ic_launcher_monochrome)
        .setContentTitle(lines?.sender ?: context.getString(R.string.notice_new_message))
        .setContentText(lines?.let { it.text ?: context.getString(R.string.notice_new_message) })
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(public)
        .setContentIntent(tap)
        .setAutoCancel(true)
        .build()
}

/**
 * Whether Android shows this app's notices: on API 33+ this is false until the user grants the
 * notification permission, on older releases until they turn the app's notices off.
 */
internal fun noticesAllowed(context: Context) =
    context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()

/**
 * Shows [mailNotice] unless the app is on screen. While Android blocks notices nothing shows; the
 * Settings notice card asks for them.
 */
internal fun postMailNotice(context: Context, lines: NoticeLines?) {
    val state = ActivityManager.RunningAppProcessInfo().also { ActivityManager.getMyMemoryState(it) }
    if (state.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND) return
    if (!noticesAllowed(context)) return
    context.getSystemService(NotificationManager::class.java).notify(MAIL_NOTICE_ID, mailNotice(context, lines))
}
