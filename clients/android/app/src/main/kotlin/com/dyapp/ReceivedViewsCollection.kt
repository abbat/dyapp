package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.LocalCollectionEntry
import uniffi.dyapp.ReceivedProfileView

/** Only an explicit row opens a profile; reading this collection never sends a view or fetches profiles. */
@Composable
internal fun ReceivedProfileViews(active: Opened.Ready) {
    val open = LocalSharedProfile.current
    ReceivedViewsCollection(
        active,
        { withContext(Dispatchers.IO) { RustBridge.withChats(active) { it.receivedProfileViews() } } },
        open
    )
}

/** Keep known rows after a read failure and retire captured actions with their original opening. */
@Composable
internal fun ReceivedViewsCollection(
    owner: Any,
    read: suspend () -> List<ReceivedProfileView>,
    open: (String) -> Unit,
) = key(owner) {
    val state = remember { ReceivedViewsState() }
    val backend = remember { read }
    val scope = rememberCoroutineScope()
    DisposableEffect(state) { onDispose { state.retire() } }
    LaunchedEffect(state) { state.load(backend) }
    val refresh: () -> Unit = { scope.launch { state.load(backend) } }
    Caption(stringResource(R.string.profile_views_hint))
    if (state.failed) {
        Caption(
            stringResource(R.string.profile_views_failed),
            Modifier.testTag("received-views-error").semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
    ReceivedViewsRows(state, refresh, open)
}

@Composable
private fun ReceivedViewsRows(state: ReceivedViewsState, refresh: () -> Unit, open: (String) -> Unit) {
    val rows = state.rows
    val entries = remember(rows) { rows?.map { LocalCollectionEntry(it.peerId, it.utcDate) } }
    if (rows == null) {
        if (!state.failed) Caption(stringResource(R.string.view_loading), Modifier.testTag("received-views-loading"))
        TextButton(refresh, Modifier.testTag("collection-refresh"), enabled = !state.busy) {
            Text(stringResource(R.string.collection_refresh))
        }
    } else {
        LocalCollectionView(state, entries, if (state.busy) null else refresh) { entry, number ->
            val onOpen = { if (state.admits(entry.target)) open(entry.target) }
            ReceivedViewRow(entry, number, !state.busy, onOpen)
        }
        if (rows.isEmpty()) {
            Caption(stringResource(R.string.profile_views_empty), Modifier.testTag("received-views-empty"))
        }
    }
}

/** Reuse the full-identifier row and stack the date/action at the current width. */
@Composable
internal fun ReceivedViewRow(entry: LocalCollectionEntry, number: Int, enabled: Boolean, open: () -> Unit) {
    val action = stringResource(R.string.profile_views_open_named, shortPeerId(entry.target))
    PeopleCollectionRow(entry.target, number) { _, _ ->
        Column(Modifier.fillMaxWidth()) {
            Caption(
                stringResource(R.string.profile_views_date, entry.label),
                Modifier.testTag("received-view-date-$number")
            )
            TextButton(
                open,
                Modifier.fillMaxWidth().testTag("received-view-open-$number").applicationClick(open)
                    .semantics { contentDescription = action },
                enabled = enabled
            ) { Text(stringResource(R.string.profile_views_open)) }
        }
    }
}
