package com.dyapp

import android.app.Notification
import android.app.NotificationManager
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatNotice

/** Real registry, JobScheduler, native mail and OS events; no Activity or injected receive backend. */
class WorkerStickerNoticeTests {
    @Test
    fun backgroundStickerPostsPrivateOsNoticeButMutedAndOffPassesStayQuiet() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val notifications = context.getSystemService(NotificationManager::class.java)
        check(noticesAllowed(context)) { "The runtime fixture must grant OS notifications" }
        check(notifications.activeNotifications.none { it.id == MAIL_NOTICE_ID }) {
            "A pre-existing app notification must not be overwritten by this test"
        }
        workerTestPolicy()
        WorkerStickerNoticePair(context).use { pair ->
            pair.prepare()
            pair.retainPrivateText()
            requirePrivateOsPost(pair)
            pair.setMuted(true)
            requireQuietReceipt(pair, pair.sendSticker())
            pair.setMuted(false)
            RustBridge.withChats(pair.owner) { it.setNotice(ChatNotice.OFF) }
            requireQuietReceipt(pair, pair.sendSticker())
        }
    }

    private fun requireQuietReceipt(pair: WorkerStickerNoticePair, id: ByteArray) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        ScheduledMailNotice(context).use { job ->
            job.schedule()
            job.awaitCompletion()
            assertEquals(0, job.attempts.size)
            pair.requireReceived(id)
        }
    }

    private fun requirePrivateOsPost(pair: WorkerStickerNoticePair) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val notifications = context.getSystemService(NotificationManager::class.java)
        val body = "\uD83D\uDE3A " + context.getString(R.string.notice_sticker)
        val sent = pair.sendSticker()
        val job = ScheduledMailNotice(context)
        try {
            job.use {
                val actual = it.awaitOsPost(body)
                it.awaitCompletion()
                val attempt = it.attempts.single()
                assertEquals(pair.owner.slot.id, attempt.slot)
                assertEquals(pair.owner.peerId, attempt.peer)
                assertEquals(body, attempt.notice.extras.getCharSequence(Notification.EXTRA_TEXT)?.toString())
                requirePrivatePayload(actual, body)
                assertTrue(notifications.activeNotifications.any { posted -> posted.id == MAIL_NOTICE_ID })
                pair.requireReceived(sent)
            }
        } finally {
            if (job.attempts.any { it.slot == pair.owner.slot.id && it.peer == pair.owner.peerId }) {
                notifications.cancel(MAIL_NOTICE_ID)
            }
        }
    }

    private fun requirePrivatePayload(actual: Notification, body: String) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals(body, actual.extras.getCharSequence(Notification.EXTRA_TEXT)?.toString())
        assertEquals(Notification.VISIBILITY_PRIVATE, actual.visibility)
        val public = checkNotNull(actual.publicVersion)
        assertEquals(context.getString(R.string.app_name), public.extras.getString(Notification.EXTRA_TITLE))
        assertNull(public.extras.getCharSequence(Notification.EXTRA_TEXT))
        assertNull(actual.getLargeIcon())
    }
}
