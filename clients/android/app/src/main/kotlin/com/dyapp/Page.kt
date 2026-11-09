package com.dyapp

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

/**
 * A full-screen page without the app bars: left-aligned [content] from the top and stacked
 * [actions] at the bottom. The whole page scrolls, so large text never hides an action.
 */
@Composable
internal fun Page(actions: @Composable ColumnScope.() -> Unit, content: @Composable ColumnScope.() -> Unit) =
    BoxWithConstraints(Modifier.fillMaxSize()) {
        Column(
            Modifier.verticalScroll(rememberScrollState()).heightIn(min = maxHeight).fillMaxWidth().padding(24.dp),
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            Column(Modifier.fillMaxWidth(), Arrangement.spacedBy(16.dp), content = content)
            Column(Modifier.fillMaxWidth().padding(top = 24.dp), Arrangement.spacedBy(8.dp), content = actions)
        }
    }

@Composable
internal fun PageTitle(id: Int, modifier: Modifier = Modifier) =
    Text(stringResource(id), modifier, style = MaterialTheme.typography.headlineMedium)

@Composable
internal fun Caption(text: String, modifier: Modifier = Modifier) = Text(
    text,
    modifier,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
    style = MaterialTheme.typography.bodySmall
)

/** A group of fields on a scrolling screen. */
@Composable
internal fun FormCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) = Surface(
    modifier.fillMaxWidth(),
    RoundedCornerShape(12.dp),
    MaterialTheme.colorScheme.surfaceContainer
) {
    Column(Modifier.padding(16.dp), Arrangement.spacedBy(12.dp), content = content)
}

@Composable
internal fun CardTitle(id: Int) = Text(
    stringResource(id),
    Modifier.semantics { heading() },
    style = MaterialTheme.typography.titleMedium
)

/** A failure on its screen: the reason, with the technical [details] to copy behind a toggle. */
@Composable
@Suppress("LongParameterList") // Every screen tags its own status, details and copy button.
internal fun ErrorNotice(text: String, textTag: String, details: String, detailsTag: String, copyTag: String) = Surface(
    Modifier.fillMaxWidth(),
    RoundedCornerShape(12.dp),
    MaterialTheme.colorScheme.errorContainer
) {
    Column(Modifier.padding(16.dp), Arrangement.spacedBy(8.dp)) {
        Text(text, Modifier.testTag(textTag).semantics { liveRegion = LiveRegionMode.Polite })
        var expanded by rememberSaveable(details) { mutableStateOf(false) }
        TextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.details)) }
        if (expanded) {
            Text(details, Modifier.testTag(detailsTag), fontFamily = FontFamily.Monospace)
            CopyButton(details, copyTag)
        }
    }
}

/** Copies [text]; Android shows its own clipboard confirmation from version 13. */
@Composable
internal fun CopyButton(text: String, tag: String) {
    val context = LocalContext.current
    val appName = stringResource(R.string.app_name)
    TextButton(
        onClick = {
            context.getSystemService(ClipboardManager::class.java)
                .setPrimaryClip(ClipData.newPlainText(appName, text))
        },
        Modifier.testTag(tag)
    ) { Text(stringResource(R.string.action_copy)) }
}
