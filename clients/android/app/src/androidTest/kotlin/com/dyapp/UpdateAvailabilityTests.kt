package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

class UpdateAvailabilityTests {
    @get:Rule
    val compose = createComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun currentUsesTheInstalledVersionAndAvailableUsesTheCoreOffer() {
        var shown by mutableStateOf<MobileUpdateState?>(null)
        compose.setContent { UpdateAvailabilityLabel(shown) }
        compose.runOnIdle { shown = snapshot(MobileUpdateAvailability.CURRENT) }
        awaitText(context.getString(R.string.update_current, "1.0.0"))
        for ((delivery, label) in listOf(
            MobileUpdateDelivery.PACKAGE to R.string.update_available_package,
            MobileUpdateDelivery.BUILD_FROM_SOURCE to R.string.update_available_source,
        )) {
            compose.runOnIdle { shown = snapshot(MobileUpdateAvailability.AVAILABLE, delivery) }
            awaitText(context.getString(label, "2.0.0"))
        }
    }

    @Test
    fun absentUnsupportedOrIncompleteMetadataNeverClaimsCurrentOrAnInstallablePackage() {
        var shown by mutableStateOf<MobileUpdateState?>(null)
        compose.setContent { UpdateAvailabilityLabel(shown) }
        awaitText(context.getString(R.string.update_not_published))
        for (availability in listOf(
            MobileUpdateAvailability.NEVER_CHECKED,
            MobileUpdateAvailability.NOT_PUBLISHED,
            MobileUpdateAvailability.DESTINATION_UNAVAILABLE,
        )) {
            compose.runOnIdle { shown = snapshot(availability) }
            awaitText(context.getString(R.string.update_not_published))
        }
        compose.runOnIdle { shown = snapshot(MobileUpdateAvailability.AVAILABLE) }
        awaitText(context.getString(R.string.update_metadata_failed))
        compose.runOnIdle { shown = snapshot(MobileUpdateAvailability.AVAILABLE, MobileUpdateDelivery.STORE) }
        awaitText(context.getString(R.string.update_not_published))
    }

    private fun awaitText(text: String) = compose.awaitNode(hasTestTag("update-state") and hasText(text))

    private fun snapshot(
        availability: MobileUpdateAvailability,
        delivery: MobileUpdateDelivery? = null
    ): MobileUpdateState {
        val offer = delivery?.let {
            MobileUpdateOffer("2.0.0", it, "https://github.com/abbat/dyapp/releases/tag/v2.0.0")
        }
        val success = if (availability == MobileUpdateAvailability.NEVER_CHECKED) null else 100uL
        return MobileUpdateState(
            installedVersion = "1.0.0",
            interval = MobileUpdateInterval.DAY,
            lastAttempt = success,
            lastSuccess = success,
            inFlight = false,
            availability = availability,
            offer = offer,
            notice = offer,
        )
    }
}
