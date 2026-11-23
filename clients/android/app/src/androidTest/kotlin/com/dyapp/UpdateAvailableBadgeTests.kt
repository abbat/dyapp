package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateNoticeAction
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState
import java.util.concurrent.ConcurrentLinkedQueue

private val badgeOffer = MobileUpdateOffer(
    "0.0.2",
    MobileUpdateDelivery.PACKAGE,
    "https://github.com/abbat/dyapp/releases/tag/v0.0.2",
)

private fun badgeSnapshot() = MobileUpdateState(
    installedVersion = "0.0.1",
    interval = MobileUpdateInterval.DAY,
    lastAttempt = 1uL,
    lastSuccess = 1uL,
    inFlight = false,
    availability = MobileUpdateAvailability.AVAILABLE,
    offer = badgeOffer,
    notice = badgeOffer,
)

class UpdateAvailableBadgeTests {
    @get:Rule
    val compose = createComposeRule()

    private fun description(version: String): String =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.update_available_badge, version)

    @Test
    fun followsTheCoreOfferAndKeepsTheVersionAccessibleAfterDismissal() {
        val snapshot = badgeSnapshot()
        val shown = mutableStateOf<MobileUpdateState?>(null)
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalClientUpdateState provides shown.value) { UpdateAvailableBadge() }
            }
        }
        compose.onNodeWithTag("updates-badge").assertDoesNotExist()
        compose.runOnIdle { shown.value = snapshot }
        compose.onNodeWithTag("updates-badge").assertContentDescriptionEquals(description("0.0.2"))
        compose.runOnIdle { shown.value = snapshot.copy(notice = null) }
        compose.onNodeWithTag("updates-badge").assertContentDescriptionEquals(description("0.0.2"))
        compose.runOnIdle {
            shown.value = snapshot.copy(offer = snapshot.offer?.copy(version = "0.0.3"))
        }
        compose.onNodeWithTag("updates-badge").assertContentDescriptionEquals(description("0.0.3"))
        compose.runOnIdle { shown.value = snapshot.copy(availability = MobileUpdateAvailability.CURRENT, offer = null) }
        compose.onNodeWithTag("updates-badge").assertDoesNotExist()
    }

    @Test
    fun theActualNoticeHostKeepsTheBadgeAfterNativeLaterAcknowledgement() {
        val owner = SupervisorJob()
        val snapshot = badgeSnapshot()
        val versions = ConcurrentLinkedQueue<String>()
        val actions = UpdateNoticeActions(CoroutineScope(Dispatchers.Main.immediate + owner), { snapshot }) {
            versions.add(it)
            MobileUpdateNoticeAction(snapshot.copy(notice = null), true)
        }
        try {
            compose.setContent {
                MaterialTheme { UpdateNoticeContent(actions, true) { UpdateAvailableBadge() } }
            }
            compose.runOnIdle { actions.initialize() }
            compose.awaitNode(hasTestTag("update-notice"))
            compose.onNodeWithTag("update-notice-later").performClick()
            compose.waitUntil(5_000) { !actions.state.value.pending && actions.state.value.snapshot?.notice == null }
            compose.onNodeWithTag("update-notice").assertDoesNotExist()
            compose.onNodeWithTag("updates-badge").assertContentDescriptionEquals(description("0.0.2"))
            assertEquals(listOf("0.0.2"), versions.toList())
        } finally {
            owner.cancel()
        }
    }
}
