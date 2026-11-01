package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.dyapp.FoundProfile

// One verified profile: the short Peer ID, its possible age and country; the page has the rest.
@Composable
internal fun SearchResultRow(found: FoundProfile, tag: String, open: () -> Unit) {
    val draft = found.draft.draft()
    val locale = LocalConfiguration.current.locales[0]
    val summary = listOfNotNull(
        possibleAgeOnUtcToday(draft.values),
        draft.country.takeIf { it.isNotEmpty() }?.let { countryName(it, locale) },
    ).joinToString(", ").ifEmpty { stringResource(R.string.profile_preview_empty) }
    Surface(
        onClick = open,
        modifier = Modifier.fillMaxWidth().testTag(tag),
        shape = RoundedCornerShape(12.dp),
        color = MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Row(Modifier.padding(12.dp), Arrangement.spacedBy(12.dp), Alignment.CenterVertically) {
            Avatar(found.peerId)
            Column {
                Text(shortPeerId(found.peerId), style = MaterialTheme.typography.titleSmall)
                Caption(summary)
            }
        }
    }
}
