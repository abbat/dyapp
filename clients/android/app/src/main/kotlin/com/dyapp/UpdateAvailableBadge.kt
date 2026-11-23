package com.dyapp

import androidx.compose.material3.Badge
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics

/** Dismissal affects the notice, while the shared core offer remains authoritative for Settings. */
@Composable
internal fun UpdateAvailableBadge() {
    val offer = LocalClientUpdateState.current?.offer ?: return
    val description = stringResource(R.string.update_available_badge, offer.version)
    Badge(Modifier.testTag("updates-badge").semantics { contentDescription = description })
}
