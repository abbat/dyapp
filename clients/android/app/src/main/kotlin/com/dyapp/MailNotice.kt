package com.dyapp

import android.Manifest
import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import uniffi.dyapp.ChatSession

private const val MAIL_CHANNEL = "mail"

// One fixed id, so each notice replaces the previous one.
internal const val MAIL_NOTICE_ID = 1

/** What a notice counts, as on desktop: unread items and unanswered likes, muted peers left out. */
internal fun noticeCount(chats: ChatSession): ULong {
    val (muted, rows) = chats.chatRows().partition { it.muted }
    val mutedPeers = muted.mapTo(HashSet()) { it.peerId }
    return chatsBadge(rows) + chats.likedYou().count { it !in mutedPeers }.toULong()
}

/**
 * The notice with the fixed "New message" text only, so a lock screen shows neither a name nor a
 * message; the public version for a secure lock screen carries just the app name.
 */
internal fun mailNotice(context: Context): Notification {
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
        .setContentTitle(context.getString(R.string.notice_new_message))
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(public)
        .setContentIntent(tap)
        .setAutoCancel(true)
        .build()
}

/**
 * Shows [mailNotice] unless the app is on screen. Without the notification permission (asked on
 * API 33+ by the planned notice setting) nothing shows.
 */
internal fun postMailNotice(context: Context) {
    val state = ActivityManager.RunningAppProcessInfo().also { ActivityManager.getMyMemoryState(it) }
    if (state.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND) return
    if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
        return
    }
    context.getSystemService(NotificationManager::class.java).notify(MAIL_NOTICE_ID, mailNotice(context))
}
