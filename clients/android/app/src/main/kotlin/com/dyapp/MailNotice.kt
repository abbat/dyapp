package com.dyapp

import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ChatState

private const val MAIL_CHANNEL = "mail"

// One fixed id, so each notice replaces the previous one.
internal const val MAIL_NOTICE_ID = 1

// The most of a message a notice shows; the rest waits in the app.
private const val NOTICE_TEXT_CHARS = 200L

/** What a notice counts, as on desktop: unread items and unanswered likes, muted peers left out. */
internal fun noticeCount(chats: ChatSession, all: List<ChatRow> = chats.chatRows()): ULong {
    val (muted, rows) = all.partition { it.muted }
    val mutedPeers = muted.mapTo(HashSet()) { it.peerId }
    return chatsBadge(rows) + chats.likedYou().count { it !in mutedPeers }.toULong()
}

/** The sender's short Peer ID and, for [ChatNotice.TEXT], the message a notice adds. */
internal data class NoticeLines(val sender: String, val text: String?)

/**
 * The lines [notice] allows for the newest unmuted row with more unread than in [before]; null keeps
 * the fixed text. Only an open chat adds them: a request or a like would put a stranger's words on
 * the screen unasked.
 */
internal fun noticeLines(notice: ChatNotice, before: List<ChatRow>, after: List<ChatRow>): NoticeLines? {
    val adds = notice == ChatNotice.NAME || notice == ChatNotice.TEXT
    // ponytail: the FFI row has no attachment, so a newer unread attachment shows the last unread text.
    val row = after.firstOrNull { row ->
        !row.muted && row.unreadCount > before.find { it.peerId == row.peerId }?.unreadCount ?: 0uL
    }?.takeIf { adds && it.state == ChatState.OPEN } ?: return null
    val text = row.lastMessage?.takeIf { notice == ChatNotice.TEXT && it.unread }?.text
    return NoticeLines(shortPeerId(row.peerId), text?.let(::oneLine)?.ifBlank { null })
}

// At most NOTICE_TEXT_CHARS code points on one line: a peer chooses this text.
private fun oneLine(text: String) = text.codePoints().limit(NOTICE_TEXT_CHARS).toArray()
    .joinToString("") { if (Character.isISOControl(it)) " " else String(Character.toChars(it)) }

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
