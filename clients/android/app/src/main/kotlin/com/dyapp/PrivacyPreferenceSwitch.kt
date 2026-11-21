package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

@Composable
internal fun PrivacyPreferenceRow(active: Opened.Ready, kind: PrivacyPreferenceKind) =
    PrivacyPreferenceSwitch(active, kind, privacyPreferenceCalls(active, kind))

@Composable
internal fun PrivacyPreferenceSwitch(owner: Any, kind: PrivacyPreferenceKind, calls: PrivacyPreferenceCalls) =
    key(owner, kind) {
        val state = remember { PrivacyPreferenceState() }
        val backend = remember { calls }
        val scope = rememberCoroutineScope()
        DisposableEffect(state) { onDispose { state.retire() } }
        LaunchedEffect(Unit) { state.complete(backend.resolve(null)) }
        val shown = state.revision
        val run = { target: Boolean? ->
            if (state.begin(target, shown)) {
                scope.launch { state.complete(backend.resolve(target)) }
            }
        }
        Column {
            PreferenceToggle(kind, state) { run(it) }
            kind.hint?.let { Caption(stringResource(it)) }
            PreferenceStatus(kind, state) { run(null) }
        }
    }

@Composable
private fun PreferenceToggle(kind: PrivacyPreferenceKind, state: PrivacyPreferenceState, save: (Boolean) -> Unit) {
    val displayed = state.value
    val action = { save(displayed != true) }
    val unknown = stringResource(R.string.privacy_preference_unknown)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag(kind.tag).applicationClick(action)
            .toggleable(displayed == true, enabled = state.editable(), role = Role.Switch, onValueChange = { action() })
            .semantics { if (displayed == null || state.failed) stateDescription = unknown },
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(stringResource(kind.title), Modifier.weight(1f))
        Switch(displayed == true, onCheckedChange = null, enabled = state.editable())
    }
}

@Composable
private fun PreferenceStatus(kind: PrivacyPreferenceKind, state: PrivacyPreferenceState, retry: () -> Unit) {
    if (state.busy) Caption(stringResource(R.string.privacy_preference_loading))
    if (state.failed) {
        Caption(
            stringResource(kind.failure),
            Modifier.testTag(kind.statusTag + "-error").semantics { liveRegion = LiveRegionMode.Polite }
        )
        TextButton(retry, Modifier.testTag(kind.statusTag + "-retry").applicationClick(retry), enabled = !state.busy) {
            Text(stringResource(R.string.privacy_preference_retry))
        }
    }
}
