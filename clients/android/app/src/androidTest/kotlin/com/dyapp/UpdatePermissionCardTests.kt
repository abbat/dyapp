package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

class UpdatePermissionCardTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun onlyASupportedAvailableOfferExposesPermissionControls() {
        var shown by mutableStateOf<MobileUpdateState?>(null)
        compose.setContent { UpdateNoticePermission(Unit, shown) }
        compose.onNodeWithTag("update-notice-allow").assertDoesNotExist()
        for (availability in listOf(
            MobileUpdateAvailability.NEVER_CHECKED,
            MobileUpdateAvailability.NOT_PUBLISHED,
            MobileUpdateAvailability.DESTINATION_UNAVAILABLE,
            MobileUpdateAvailability.CURRENT,
        )) {
            compose.runOnIdle { shown = snapshot().copy(availability = availability) }
            compose.onNodeWithTag("update-notice-allow").assertDoesNotExist()
        }
        compose.runOnIdle { shown = snapshot().copy(offer = null) }
        compose.onNodeWithTag("update-notice-allow").assertDoesNotExist()
        compose.runOnIdle {
            shown = snapshot().let { it.copy(offer = it.offer?.copy(delivery = MobileUpdateDelivery.STORE)) }
        }
        compose.onNodeWithTag("update-notice-allow").assertDoesNotExist()
        for (delivery in listOf(MobileUpdateDelivery.PACKAGE, MobileUpdateDelivery.BUILD_FROM_SOURCE)) {
            compose.runOnIdle {
                shown = snapshot().let { it.copy(offer = it.offer?.copy(delivery = delivery)) }
            }
            compose.awaitNode(hasTestTag("update-notice-permission"))
            compose.onNodeWithTag("update-notice-allow").assertExists()
        }
        compose.runOnIdle { shown = null }
        compose.onNodeWithTag("update-notice-allow").assertDoesNotExist()
    }

    private fun snapshot() = MobileUpdateState(
        installedVersion = "0.0.1",
        interval = MobileUpdateInterval.DAY,
        lastAttempt = 100uL,
        lastSuccess = 100uL,
        inFlight = false,
        availability = MobileUpdateAvailability.AVAILABLE,
        offer = MobileUpdateOffer(
            "0.0.2",
            MobileUpdateDelivery.PACKAGE,
            "https://github.com/abbat/dyapp/releases/tag/v0.0.2"
        ),
        notice = null,
    )
}
