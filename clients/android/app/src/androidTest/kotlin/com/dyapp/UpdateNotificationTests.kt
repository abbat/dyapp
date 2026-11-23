package com.dyapp

import android.app.Notification
import android.content.Intent
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateOffer

class UpdateNotificationTests {
    private val offer = MobileUpdateOffer(
        "2.0.0",
        MobileUpdateDelivery.PACKAGE,
        "https://github.com/abbat/dyapp/releases/tag/v2.0.0"
    )

    @Test
    fun updateNoticeHasItsOwnChannelAndImmutableInformationAction() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val notice = updateNotification(context, offer)
        assertNotEquals(MAIL_NOTICE_ID, UPDATE_NOTICE_ID)
        assertEquals(UPDATE_NOTICE_CHANNEL, notice.channelId)
        assertTrue(checkNotNull(notice.contentIntent).isImmutable)
        assertTrue(checkNotNull(notice.contentIntent).isActivity)
        assertEquals(
            context.getString(R.string.update_available_badge, offer.version),
            notice.extras.getCharSequence(Notification.EXTRA_TEXT)
        )
        assertEquals(Notification.VISIBILITY_PRIVATE, notice.visibility)
        val public = checkNotNull(notice.publicVersion)
        assertEquals(context.getString(R.string.app_name), public.extras.getString(Notification.EXTRA_TITLE))
        assertNull(public.extras.getCharSequence(Notification.EXTRA_TEXT))
    }

    @Test
    fun tapUsesExactlyTheCorePageWithoutAnInstallerOrIdentityPayload() {
        val intent = updateNotificationIntent(offer)
        assertEquals(Intent.ACTION_VIEW, intent.action)
        assertEquals(offer.releasePage, intent.dataString)
        assertTrue(intent.flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0)
        assertNull(intent.component)
        assertNull(intent.type)
        assertNull(intent.extras)
    }
}
