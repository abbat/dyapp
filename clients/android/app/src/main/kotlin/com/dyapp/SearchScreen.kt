package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.ProfileSearchFailure
import uniffi.dyapp.ProfileSearchResult
import uniffi.dyapp.ProfileSearcher
import uniffi.dyapp.profileScales

private const val MS_PER_SECOND = 1000uL

/** Nothing asked yet, a search running, its verified result, or the failure text with its details. */
internal sealed interface SearchState {
    data object Idle : SearchState
    data object Loading : SearchState
    data class Done(val result: ProfileSearchResult) : SearchState
    data class Failed(val text: Int, val details: String) : SearchState
}

private fun searchError(error: Throwable) = when {
    "notConnected" in error.toString() -> R.string.search_not_connected
    "invalid search age" in error.toString() -> R.string.search_age_invalid
    "invalid search height" in error.toString() -> R.string.search_height_invalid
    "mutual unavailable" in error.toString() -> R.string.search_mutual_unavailable
    else -> R.string.search_failed
}

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

private const val STATUS_TAG = "search-status"
private const val FAILURES_TAG = "search-failures"
private val STATUS = Modifier.testTag(STATUS_TAG)

@Composable
private fun SearchFailures(result: ProfileSearchResult) {
    Column(Modifier.testTag(FAILURES_TAG)) {
        for (failure in result.failures) Caption(failureText(failure))
    }
}

@Composable
private fun SearchLoading(cancel: () -> Unit) {
    Caption(stringResource(R.string.search_loading), STATUS.semantics { liveRegion = LiveRegionMode.Polite })
    repeat(2) {
        Surface(Modifier.fillMaxWidth().height(72.dp), RoundedCornerShape(12.dp),
            MaterialTheme.colorScheme.surfaceContainer) {}
    }
    OutlinedButton(onClick = cancel, modifier = Modifier.fillMaxWidth().testTag("search-cancel")) {
        Text(stringResource(R.string.search_cancel))
    }
}

private val FIX_THE_FIELDS =
    setOf(R.string.search_age_invalid, R.string.search_height_invalid, R.string.search_mutual_unavailable)

@Composable
private fun SearchFailed(failed: SearchState.Failed, run: () -> Unit, openNetwork: () -> Unit) {
    ErrorNotice(stringResource(failed.text), STATUS_TAG, failed.details, FAILURES_TAG, "copy-search-details")
    // A range nothing fits or a missing own age needs the fields fixed, not a connection or the same query again.
    if (failed.text !in FIX_THE_FIELDS) {
        OutlinedButton(onClick = openNetwork, modifier = Modifier.fillMaxWidth().testTag("search-open-network")) {
            Text(stringResource(R.string.search_open_network))
        }
        OutlinedButton(onClick = run, modifier = Modifier.fillMaxWidth().testTag("search-retry")) {
            Text(stringResource(R.string.search_retry))
        }
    }
}

// The advice points at the filters, so this state offers no other sample.
@Composable
private fun SearchEmpty(result: ProfileSearchResult) {
    Column(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp), Alignment.CenterHorizontally) {
        Icon(painterResource(R.drawable.ic_nav_search), null)
        Text(stringResource(R.string.search_empty), STATUS.semantics {
            heading()
            liveRegion = LiveRegionMode.Polite
        }, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Caption(stringResource(R.string.search_empty_text))
        SearchFailures(result)
    }
}

// Typed failures, then each verified profile, which opens its page.
@Composable
private fun SearchFound(result: ProfileSearchResult, run: () -> Unit, openProfile: (FoundProfile) -> Unit) {
    val count = result.profiles.size
    Column(Modifier.testTag("search-results"), Arrangement.spacedBy(12.dp)) {
        Caption(stringResource(if (result.partial) R.string.search_partial else R.string.search_results, count),
            STATUS.semantics { liveRegion = LiveRegionMode.Polite })
        SearchFailures(result)
        FetchCardPhotos(result.profiles)
        for ((index, found) in result.profiles.withIndex()) {
            SearchResultRow(found, "result-${index + 1}") { openProfile(found) }
        }
        OutlinedButton(onClick = run, modifier = Modifier.fillMaxWidth().testTag("search-again")) {
            Text(stringResource(R.string.search_again))
        }
    }
}

// One sample off the UI thread with the shown filters; the offered fields then pick the filters.
private suspend fun sample(
    opened: Opened.Ready,
    searcher: Result<ProfileSearcher>,
    filters: SearchFilters,
): SearchState {
    val conditions = runCatching { filters.conditions() }
    val result = withContext(Dispatchers.IO) {
        runCatching {
            val query = conditions.getOrThrow() + filters.mutualConditions(opened)
            RustBridge.withKey(opened) { _, key ->
                searcher.getOrThrow().search(key, query, filters.employer.employer(), 0u)
            }
        }
    }
    result.onSuccess { filters.offered = it.offered }
    return result.fold({ SearchState.Done(it) }, { SearchState.Failed(searchError(it), it.toString()) })
}

/** One identity's search; the shell owns it so the results outlive the pages opened over them. */
internal class SearchSession(private val opened: Opened.Ready, private val scope: CoroutineScope) {
    // The searcher keeps verified versions, so a later sample never shows an older one.
    private val searcher = runCatching { ProfileSearcher(opened.slot.dir.absolutePath) }
    private var job: Job? = null
    val photos = PeerPhotos(searcher.getOrNull(), opened)
    val filters = SearchFilters(profileScales())
    var state by mutableStateOf<SearchState>(SearchState.Idle)
        private set

    fun run() {
        state = SearchState.Loading
        job = scope.launch { state = sample(opened, searcher, filters) }
    }

    fun cancel() {
        job?.cancel()
        state = SearchState.Idle
    }

    fun close() {
        searcher.getOrNull()?.close()
    }
}

@Composable
internal fun rememberSearchSession(opened: Opened.Ready): SearchSession {
    val scope = rememberCoroutineScope()
    val session = remember(opened.slot.id) { SearchSession(opened, scope) }
    DisposableEffect(session) { onDispose { session.close() } }
    return session
}

/**
 * Verified sample search over the active network; the nodes pick the sample, filters appear for the
 * fields they offer, and only profiles the shared core verified are listed. Retry or another sample
 * runs the same query again. Cancel abandons the answer; the blocking call itself runs to its end.
 */
@Composable
internal fun SearchScreen(search: SearchSession, openNetwork: () -> Unit, openProfile: (FoundProfile) -> Unit) {
    val state = search.state
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Caption(stringResource(R.string.search_sample))
        FormCard {
            SearchFilterFields(search.filters)
            Button(onClick = search::run, enabled = state != SearchState.Loading,
                modifier = Modifier.fillMaxWidth().testTag("run-search")) {
                Text(stringResource(R.string.search_run))
            }
        }
        when (val shown = state) {
            SearchState.Idle -> Caption(stringResource(R.string.search_idle), STATUS)
            SearchState.Loading -> SearchLoading(search::cancel)
            is SearchState.Failed -> SearchFailed(shown, search::run, openNetwork)
            is SearchState.Done -> shown.result.let {
                if (it.profiles.isEmpty()) SearchEmpty(it) else SearchFound(it, search::run, openProfile)
            }
        }
    }
}
