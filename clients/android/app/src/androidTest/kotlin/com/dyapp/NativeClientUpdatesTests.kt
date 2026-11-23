package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdatePlatform
import java.io.File
import java.util.UUID

/** Actual JNI metadata and packaged-version contract; no HTTP or emulator-specific timing. */
class NativeClientUpdatesTests {
    private val root = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "native-client-updates-${UUID.randomUUID()}",
    ).apply { check(mkdir()) }

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    @Test
    fun metadataMatchesThePackagedVersionAndIntervalsSurviveAnotherHandle() {
        MobileClientUpdates(root.absolutePath, MobileUpdatePlatform.ANDROID).use { client ->
            val fresh = client.readState()
            assertEquals(BuildConfig.VERSION_NAME, fresh.installedVersion)
            assertEquals(MobileUpdateAvailability.NEVER_CHECKED, fresh.availability)
            assertFalse(fresh.inFlight)
            assertNull(fresh.lastSuccess)
            assertNull(fresh.offer)
            assertNull(fresh.notice)
            assertNull(client.claimPendingNotification())
            val dismissal = client.dismissNotice("1.0.0")
            assertFalse(dismissal.applied)
            assertEquals(fresh, dismissal.state)
            val claim = client.claimNotification("1.0.0")
            assertFalse(claim.applied)
            assertEquals(fresh, claim.state)
            for (interval in listOf(
                MobileUpdateInterval.SIX_HOURS,
                MobileUpdateInterval.TWELVE_HOURS,
                MobileUpdateInterval.DAY,
                MobileUpdateInterval.WEEK,
            )) {
                assertEquals(interval, client.setInterval(interval).interval)
                MobileClientUpdates(root.absolutePath, MobileUpdatePlatform.ANDROID).use { reopened ->
                    assertEquals(interval, reopened.readState().interval)
                }
            }
        }
    }
}
