package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import uniffi.dyapp.LocalCollectionCursor
import uniffi.dyapp.LocalCollectionEntry
import uniffi.dyapp.LocalCollectionPage
import uniffi.dyapp.localCollectionPage

private class CollectionViewState {
    var query by mutableStateOf("")
    var cursor by mutableStateOf<LocalCollectionCursor?>(null)
    var present = true

    fun search(value: String) {
        if (present) {
            query = value
            cursor = null
        }
    }

    fun navigate(value: LocalCollectionCursor?) {
        if (present) cursor = value
    }
}

/** Rust owns search, counts and page bounds; this opening owns only input and navigation state. */
@Composable
internal fun LocalCollectionView(
    owner: Any,
    entries: List<LocalCollectionEntry>?,
    refresh: (() -> Unit)?,
    row: @Composable (LocalCollectionEntry, Int) -> Unit
) = key(owner) {
    val state = remember { CollectionViewState() }
    DisposableEffect(state) { onDispose { state.present = false } }
    val query = state.query
    val cursor = state.cursor
    val loaded by rememberNativeRead(entries, query, cursor) {
        entries?.let { localCollectionPage(it, query, cursor) }
    }
    val page = loaded?.getOrNull()
    CollectionSearch(state, refresh)
    when {
        loaded == null || entries == null -> {
            Caption(stringResource(R.string.view_loading))
        }
        loaded?.isFailure == true -> {
            Text(stringResource(R.string.storage_error))
        }
        page != null -> {
            CollectionNavigation(state, page)
            page.entries.forEachIndexed { index, entry -> row(entry, index + 1) }
            if (page.total > 0uL && page.matched == 0uL) Text(stringResource(R.string.collection_no_results))
        }
    }
}

@Composable
private fun CollectionSearch(state: CollectionViewState, refresh: (() -> Unit)?) {
    Column {
        OutlinedTextField(
            state.query,
            state::search,
            Modifier.fillMaxWidth().testTag("collection-query"),
            label = { Text(stringResource(R.string.collection_search)) },
            singleLine = true
        )
        TextButton(
            onClick = { if (state.present) refresh?.invoke() },
            Modifier.testTag("collection-refresh"),
            enabled = refresh != null
        ) { Text(stringResource(R.string.collection_refresh)) }
    }
}

@Composable
private fun CollectionNavigation(state: CollectionViewState, page: LocalCollectionPage) {
    val next = { state.navigate(page.next) }
    Column {
        Caption(
            stringResource(R.string.collection_count, page.matched.toLong(), page.total.toLong()),
            Modifier.testTag("collection-count").semantics { liveRegion = LiveRegionMode.Polite }
        )
        Caption(
            stringResource(R.string.collection_page, page.number.toLong(), page.pages.toLong()),
            Modifier.testTag("collection-page")
        )
        TextButton(
            onClick = { state.navigate(page.previous) },
            Modifier.testTag("collection-previous"),
            enabled = page.previous != null
        ) { Text(stringResource(R.string.collection_previous)) }
        TextButton(
            onClick = next,
            Modifier.testTag("collection-next").applicationClick(next),
            enabled = page.next != null
        ) { Text(stringResource(R.string.collection_next)) }
    }
}
