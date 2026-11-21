package com.dyapp

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

private val categories = listOf(
    SettingsPane.APPEARANCE,
    SettingsPane.PRIVACY,
    SettingsPane.NOTIFICATIONS,
    SettingsPane.PHOTOS,
    SettingsPane.ACCOUNT,
    SettingsPane.ABOUT
)

@Composable
internal fun SettingsLink(pane: SettingsPane, route: MutableState<SettingsPane>) {
    val open = { route.value = pane }
    TextButton(
        onClick = open,
        Modifier.fillMaxWidth().testTag("open-" + pane.tag).applicationClick(open)
    ) { Text(stringResource(pane.title)) }
}

/** Each pane owns its scroll position; back targets depend on the current route, never a stale callback. */
@Composable
internal fun SettingsFrame(route: MutableState<SettingsPane>, content: @Composable ColumnScope.(SettingsPane) -> Unit) {
    val pane = route.value
    val back = { route.value = route.value.parent() }
    BackHandler(enabled = pane != SettingsPane.ROOT, onBack = back)
    key(pane) {
        Column(
            Modifier.fillMaxSize().testTag(pane.tag).verticalScroll(rememberScrollState())
                .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
            Arrangement.spacedBy(12.dp)
        ) {
            if (pane == SettingsPane.ROOT) {
                categories.forEach { SettingsLink(it, route) }
            } else {
                SettingsBackBar(pane, back)
                content(pane)
            }
        }
    }
}

@Composable
private fun SettingsBackBar(pane: SettingsPane, back: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton(back, Modifier.testTag("settings-back")) {
            Icon(painterResource(R.drawable.ic_back), stringResource(R.string.lock_back))
        }
        Text(
            stringResource(pane.title),
            Modifier.testTag("settings-title").semantics { heading() },
            style = MaterialTheme.typography.titleLarge
        )
    }
}
