package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.profileScales

/** A fetched profile's gallery and public fields. */
@Composable
internal fun ShownProfile(found: FoundProfile) {
    val scales = remember { profileScales() }
    PeerGallery(found)
    FormCard(Modifier.testTag("view-fields")) {
        ProfileRows(found.draft.draft(), scales, LocalContext.current)
    }
}
