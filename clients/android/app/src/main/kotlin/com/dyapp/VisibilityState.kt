package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ProfilePublisher
import uniffi.dyapp.profileVisibility

/** What the hide switch shows; only a finished hide or publication changes it. */
internal class VisibilityState(version: ULong, hidden: Boolean) {
    var version by mutableStateOf(version)
    var hidden by mutableStateOf(hidden)
    var hiding by mutableStateOf(false)
    var failure by mutableStateOf<Int?>(null)

    /** The switch turned off: the publish section opens its dialog. */
    var publishAsked by mutableStateOf(false)

    fun shown(published: ULong) {
        version = published
        hidden = false
        failure = null
    }
}

// An unreadable state leaves the switch off and disabled, as before a first publication.
internal fun loadVisibility(opened: Opened.Ready): VisibilityState =
    runCatching { RustBridge.withKey(opened) { dir, key -> profileVisibility(dir, key) } }
        .fold({ VisibilityState(it.version, it.hidden) }, { VisibilityState(0uL, false) })

private suspend fun hide(opened: Opened.Ready, state: VisibilityState) {
    state.hiding = true
    state.failure = null
    withContext(Dispatchers.IO) {
        runCatching { RustBridge.withKey(opened) { dir, key -> ProfilePublisher(dir).use { it.hide(key) } } }
    }.fold({
        state.version = it.version
        state.hidden = it.hidden
    }, { state.failure = publishError(it) })
    state.hiding = false
}

/** The switch: on hides now, off publishes again, which needs the edits saved first. */
@Composable
internal fun HideCard(opened: Opened.Ready, state: VisibilityState, unsaved: Boolean) {
    val scope = rememberCoroutineScope()
    FormCard {
        Row(
            Modifier.fillMaxWidth().testTag("profile-hidden").toggleable(
                state.hidden,
                enabled = state.version > 0uL && !state.hiding && !(state.hidden && unsaved),
                role = Role.Switch,
            ) { if (state.hidden) state.publishAsked = true else scope.launch { hide(opened, state) } },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.profile_hide))
                Caption(stringResource(R.string.profile_hide_hint))
            }
            Switch(state.hidden, onCheckedChange = null)
        }
    }
    VisibilityNotices(state)
}

@Composable
private fun VisibilityNotices(state: VisibilityState) {
    if (state.hidden) {
        Text(
            stringResource(R.string.profile_hidden_notice),
            Modifier.testTag("profile-hidden-notice")
                .semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
    state.failure?.let {
        Text(
            stringResource(it),
            Modifier.testTag("profile-hide-error")
                .semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
}
