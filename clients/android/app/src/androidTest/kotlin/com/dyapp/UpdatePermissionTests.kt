package com.dyapp

import android.provider.Settings
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdatePermissionTests {
    @Test
    fun pendingAdmissionAndRetiredCompletionsBelongToTheirOpening() {
        val old = UpdatePermissionOpening()
        val replacement = UpdatePermissionOpening()
        var launches = 0
        var reads = 0
        old.begin(true) { launches++ }
        old.begin(true) { launches++ }
        assertEquals(1, launches)
        assertTrue(old.pending)
        assertTrue(old.asked)
        old.retire()
        old.complete { reads++ }
        old.begin(false) { launches++ }
        assertEquals(0, reads)
        assertEquals(1, launches)
        replacement.begin(false) { launches++ }
        replacement.complete { reads++ }
        replacement.complete { reads++ }
        assertEquals(2, launches)
        assertEquals(1, reads)
        assertFalse(replacement.pending)
        assertFalse(replacement.asked)
    }

    @Test
    fun failedLaunchReleasesAdmissionAndDenialDoesNotPreventExplicitSettings() {
        val opening = UpdatePermissionOpening()
        runCatching { opening.begin(true) { error("No platform handler") } }
        assertFalse(opening.pending)
        assertTrue(opening.asked)
        var read = false
        opening.begin(false) {}
        opening.complete { read = true }
        assertTrue(read)
        assertFalse(opening.pending)
    }

    @Test
    fun platformDestinationUsesActualAppPermissionAndTheDedicatedUpdateChannel() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val allowed = noticesAllowed(context)
        val intent = updateNoticeSettings(context)
        assertEquals(context.packageName, intent.getStringExtra(Settings.EXTRA_APP_PACKAGE))
        assertEquals(
            if (allowed) Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS else Settings.ACTION_APP_NOTIFICATION_SETTINGS,
            intent.action
        )
        if (allowed) assertEquals(UPDATE_NOTICE_CHANNEL, intent.getStringExtra(Settings.EXTRA_CHANNEL_ID))
        if (!allowed) assertFalse(updateNoticePermission(context))
    }
}
