package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics

/** Existing-owner recovery never replaces the profile editor or its unsaved fields. */
@Composable
internal fun SearchRecoveryCard(opened: Opened.Ready, visibility: VisibilityState) {
    val session = remember(opened) { searchRecoverySession(opened) }
    val state by session.state.collectAsState()
    LaunchedEffect(session) { session.prepare(null) }
    DisposableEffect(session) { onDispose { session.retire() } }
    LaunchedEffect(state.saved) {
        state.saved?.let { visibility.observed(it.version, it.hidden) }
    }
    if (state.retired || state.opening == null && state.failure == null) return
    SearchRecoveryContent(opened.peerId, session, state)
}

@Composable
private fun SearchRecoveryContent(peer: String, session: SearchRecoverySession, state: SearchRecoveryView) = FormCard {
    CardTitle(R.string.search_recovery_title)
    DeviceIdentifier(peer, "search-recovery-owner", R.string.nav_profile)
    Text(
        stringResource(recoveryMessage(state)),
        Modifier.testTag("search-recovery-status").semantics { liveRegion = LiveRegionMode.Polite }
    )
    if (state.saved?.searchPending != false) {
        SearchRecoveryAction(session, state)
    }
    TextButton(
        onClick = { session.retire() },
        enabled = !state.busy,
        modifier = Modifier.fillMaxWidth().testTag("search-recovery-close")
    ) {
        Text(stringResource(if (state.saved == null) R.string.remove_cancel else R.string.search_recovery_close))
    }
}

@Composable
private fun SearchRecoveryAction(session: SearchRecoverySession, state: SearchRecoveryView) {
    Button(
        onClick = {
            val opening = state.opening
            when {
                state.saved?.searchPending == true && opening != null -> session.retry(opening)
                state.failure != null -> session.prepare(opening)
                opening != null -> session.confirm(opening)
            }
        },
        enabled = !state.busy,
        modifier = Modifier.fillMaxWidth().testTag("search-recovery-action")
    ) { Text(stringResource(recoveryAction(state))) }
}

private fun recoveryMessage(state: SearchRecoveryView): Int = when {
    state.busy -> R.string.search_recovery_busy
    state.failure != null -> R.string.search_recovery_error
    state.saved?.searchPending == true -> R.string.search_recovery_pending
    state.saved != null -> R.string.search_recovery_sent
    else -> R.string.search_recovery_review
}

private fun recoveryAction(state: SearchRecoveryView): Int = when {
    state.saved?.searchPending == true -> R.string.search_recovery_retry
    state.failure != null -> R.string.search_recovery_refresh
    else -> R.string.search_recovery_confirm
}
