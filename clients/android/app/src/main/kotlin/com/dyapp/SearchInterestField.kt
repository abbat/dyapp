package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.profileInterestLimits

/** Only offered fields are shown; the shared checked query parser owns admission. */
@Composable
internal fun SearchInterestField(filters: SearchFilters) {
    val limits = remember { profileInterestLimits() }
    OutlinedTextField(
        filters.interest,
        { filters.interest = it },
        Modifier.fillMaxWidth().testTag("search-interest"),
        label = { Text(stringResource(R.string.search_interest)) },
        supportingText = {
            Text(stringResource(R.string.search_interest_hint, limits.maxTagChars.toString()))
        },
        singleLine = true
    )
}
