package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

private const val DEFAULT_INTERVAL = "update-24h"

// The check intervals in order, with their test tags.
private val intervals = listOf(
    "update-6h" to R.string.update_6h,
    "update-12h" to R.string.update_12h,
    DEFAULT_INTERVAL to R.string.update_24h,
    "update-7d" to R.string.update_7d,
)

/**
 * The Settings updates card. The verified update feed is not live yet, so every control stays
 * disabled and the card says updates for Android are not published; it sends no request.
 */
// ponytail: static until the shared checker reads the live feed; then persist the interval app-wide.
@Composable
internal fun UpdatesCard() = FormCard(Modifier.testTag("updates")) {
    CardTitle(R.string.settings_updates)
    Column(Modifier.selectableGroup()) {
        Text(stringResource(R.string.update_interval))
        intervals.forEach { (tag, label) -> ChoiceRow(tag, label, tag == DEFAULT_INTERVAL, enabled = false) {} }
    }
    Caption(stringResource(R.string.update_last_check, stringResource(R.string.update_never)))
    Caption(stringResource(R.string.update_not_published), Modifier.testTag("update-state"))
    OutlinedButton(onClick = {}, Modifier.testTag("update-check"), enabled = false) {
        Text(stringResource(R.string.update_check))
    }
}
