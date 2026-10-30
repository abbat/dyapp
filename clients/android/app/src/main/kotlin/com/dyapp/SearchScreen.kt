package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.ProfileSearchFailure
import uniffi.dyapp.ProfileSearchResult
import uniffi.dyapp.ProfileSearcher
import uniffi.dyapp.SearchEmployer
import uniffi.dyapp.profileScales

private const val MS_PER_SECOND = 1000uL

/** Nothing asked yet, a search running, its verified result, or the failure text. */
private sealed interface SearchState {
    data object Idle : SearchState
    data object Loading : SearchState
    data class Done(val result: ProfileSearchResult) : SearchState
    data class Failed(val text: Int) : SearchState
}

private fun searchError(error: Throwable) =
    if ("notConnected" in error.toString()) R.string.search_not_connected else R.string.search_failed

@Composable
private fun failureText(failure: ProfileSearchFailure) = when (failure) {
    is ProfileSearchFailure.NoNodes -> stringResource(R.string.search_failure_no_nodes)
    is ProfileSearchFailure.Unavailable -> stringResource(R.string.search_failure_unavailable)
    is ProfileSearchFailure.Timeout -> stringResource(R.string.search_failure_timeout)
    is ProfileSearchFailure.RateLimited -> stringResource(R.string.search_failure_rate_limited,
        ((failure.retryAfterMs + MS_PER_SECOND - 1uL) / MS_PER_SECOND).toString())
    is ProfileSearchFailure.Puzzle -> stringResource(R.string.search_failure_puzzle)
    is ProfileSearchFailure.Invalid -> stringResource(R.string.search_failure_invalid)
}

@Composable
private fun statusText(state: SearchState) = when (state) {
    SearchState.Idle -> stringResource(R.string.search_idle)
    SearchState.Loading -> stringResource(R.string.search_loading)
    is SearchState.Failed -> stringResource(state.text)
    is SearchState.Done -> when {
        state.result.profiles.isEmpty() -> stringResource(R.string.search_empty)
        state.result.partial -> stringResource(R.string.search_partial, state.result.profiles.size)
        else -> stringResource(R.string.search_results, state.result.profiles.size)
    }
}

// Typed failures, then each verified profile by Peer ID with its public fields.
@Composable
private fun SearchResults(result: ProfileSearchResult, scales: List<ProfileScale>) {
    for (failure in result.failures) Text(failureText(failure), Modifier.testTag("search-failure"))
    val context = LocalContext.current
    for (found in result.profiles) {
        Column(Modifier.testTag("search-result")) {
            Text(stringResource(R.string.search_peer, found.peerId), style = MaterialTheme.typography.titleSmall)
            ProfileRows(found.draft.draft(), scales, context)
        }
    }
}

// One unfiltered sample off the UI thread.
private suspend fun sample(opened: Opened.Ready, searcher: Result<ProfileSearcher>): SearchState {
    val result = withContext(Dispatchers.IO) {
        runCatching {
            RustBridge.withKey(opened) { _, key ->
                searcher.getOrThrow().search(key, emptyList(), SearchEmployer.Any, 0u)
            }
        }
    }
    return result.fold({ SearchState.Done(it) }, { SearchState.Failed(searchError(it)) })
}

/**
 * Verified sample search over the active network; the nodes pick the sample, and only profiles the
 * shared core verified are listed. Retry or another sample runs the same query again.
 */
@Composable
internal fun SearchScreen(opened: Opened.Ready, onBack: () -> Unit) {
    val scales = remember { profileScales() }
    // The searcher keeps verified versions for this screen, so a later sample never shows an older one.
    val searcher = remember(opened.slot.id) { runCatching { ProfileSearcher(opened.slot.dir.absolutePath) } }
    DisposableEffect(searcher) { onDispose { searcher.getOrNull()?.close() } }
    var state by remember(opened.slot.id) { mutableStateOf<SearchState>(SearchState.Idle) }
    val scope = rememberCoroutineScope()
    val run: () -> Unit = {
        state = SearchState.Loading
        scope.launch { state = sample(opened, searcher) }
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.search_title), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.search_sample))
        SearchActions(state, run)
        Text(statusText(state), Modifier.testTag("search-status"))
        (state as? SearchState.Done)?.let { SearchResults(it.result, scales) }
        OutlinedButton(onClick = onBack, Modifier.testTag("search-back")) {
            Text(stringResource(R.string.profile_back))
        }
    }
}

// Search, then Retry after a failure or Load another sample after a result; disabled while loading.
@Composable
private fun SearchActions(state: SearchState, run: () -> Unit) {
    val idle = state != SearchState.Loading
    Button(onClick = run, enabled = idle, modifier = Modifier.testTag("run-search")) {
        Text(stringResource(R.string.search_run))
    }
    val again = when (state) {
        is SearchState.Failed -> R.string.search_retry
        is SearchState.Done -> R.string.search_again
        else -> null
    }
    again?.let {
        OutlinedButton(onClick = run, enabled = idle, modifier = Modifier.testTag("search-again")) {
            Text(stringResource(it))
        }
    }
}
