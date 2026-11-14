package com.dyapp

import android.app.Notification
import android.app.job.JobScheduler
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MailJobTests {
    @Test
    fun theAppSchedulesAPersistedPeriodicReceiveThatNeedsANetwork() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val job = checkNotNull(context.getSystemService(JobScheduler::class.java).getPendingJob(MAIL_JOB_ID))
        assertTrue(job.isPeriodic)
        assertTrue(job.isPersisted)
        assertNotNull(job.requiredNetwork)
    }

    @Test
    fun theNoticeCarriesNeitherANameNorAText() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val notice = mailNotice(context)
        assertEquals(context.getString(R.string.notice_new_message), notice.extras.getString(Notification.EXTRA_TITLE))
        assertNull(notice.extras.getCharSequence(Notification.EXTRA_TEXT))
        assertEquals(Notification.VISIBILITY_PRIVATE, notice.visibility)
        val public = checkNotNull(notice.publicVersion)
        assertEquals(context.getString(R.string.app_name), public.extras.getString(Notification.EXTRA_TITLE))
        assertNull(public.extras.getCharSequence(Notification.EXTRA_TEXT))
        assertNotNull(notice.contentIntent)
    }

    @Test
    fun theCoreStickerBodyIsLocalizedAndThePublicVersionStillHidesIt() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val text = "\uD83D\uDE3A " + context.getString(R.string.notice_sticker)
        val admitted = uniffi.dyapp.ChatMailNotice("peer-abcdef", text)
        val notice = mailNotice(context, noticeLines(admitted))
        assertEquals(text, notice.extras.getCharSequence(Notification.EXTRA_TEXT))
        val public = checkNotNull(notice.publicVersion)
        assertEquals(context.getString(R.string.app_name), public.extras.getString(Notification.EXTRA_TITLE))
        assertNull(public.extras.getCharSequence(Notification.EXTRA_TEXT))
    }

    @Test
    fun theSettingAddsTheSenderAndTextButNotOnASecureLockScreen() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val named = mailNotice(context, NoticeLines("@abc", null))
        assertEquals("@abc", named.extras.getString(Notification.EXTRA_TITLE))
        val fixed = context.getString(R.string.notice_new_message)
        assertEquals(fixed, named.extras.getCharSequence(Notification.EXTRA_TEXT))
        val notice = mailNotice(context, NoticeLines("@abc", "hi"))
        assertEquals("hi", notice.extras.getCharSequence(Notification.EXTRA_TEXT))
        val public = checkNotNull(notice.publicVersion)
        assertEquals(context.getString(R.string.app_name), public.extras.getString(Notification.EXTRA_TITLE))
        assertNull(public.extras.getCharSequence(Notification.EXTRA_TEXT))
    }
}
