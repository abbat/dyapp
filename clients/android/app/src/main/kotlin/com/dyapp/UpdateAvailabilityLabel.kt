package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

/** Only presentation: availability, version comparison and destinations are decided by core. */
@Composable
internal fun UpdateAvailabilityLabel(state: MobileUpdateState?) {
    val text = when {
        state == null -> stringResource(R.string.update_not_published)
        state.availability == MobileUpdateAvailability.CURRENT ->
            stringResource(R.string.update_current, state.installedVersion)
        state.availability == MobileUpdateAvailability.AVAILABLE -> updateOfferText(state.offer)
        else -> stringResource(R.string.update_not_published)
    }
    Caption(text, Modifier.testTag("update-state"))
}

@Composable
private fun updateOfferText(offer: MobileUpdateOffer?): String {
    if (offer == null) return stringResource(R.string.update_metadata_failed)
    return when (offer.delivery) {
        MobileUpdateDelivery.PACKAGE -> stringResource(R.string.update_available_package, offer.version)
        MobileUpdateDelivery.BUILD_FROM_SOURCE -> stringResource(R.string.update_available_source, offer.version)
        MobileUpdateDelivery.STORE -> stringResource(R.string.update_not_published)
    }
}
