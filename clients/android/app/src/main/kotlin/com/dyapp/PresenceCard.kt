package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

private class PresencePresentation {
    var value by mutableStateOf<Boolean?>(null)
    var busy by mutableStateOf(true)
    var failed by mutableStateOf(false)

    fun editable() = value != null && !busy && !failed

    fun admits(target: Boolean?) = !busy && (target == null || editable() && value != target)

    fun complete(result: Result<Boolean>) {
        failed = result.isFailure
        result.onSuccess { value = it }
        busy = false
    }
}

@Composable
internal fun PresenceCard(active: Opened.Ready) = PresencePicker(active.slot.id, presenceCalls(active))

/** Only the selected audience is local presentation state, never the disclosure policy. */
@Composable
internal fun PresencePicker(owner: Any, calls: PresenceCalls) = key(owner) {
    val state = remember { PresencePresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.complete(backend.resolve(null)) }
    val run = { target: Boolean? ->
        if (state.admits(target)) {
            state.busy = true
            state.failed = false
            scope.launch { state.complete(backend.resolve(target)) }
        }
    }
    FormCard {
        CardTitle(R.string.presence_title)
        PresenceChoices(state, run)
        PresenceStatus(state) { run(null) }
    }
}

@Composable
private fun PresenceChoices(state: PresencePresentation, pick: (Boolean) -> Unit) {
    Column(Modifier.selectableGroup().testTag("presence-audience")) {
        for ((enabled, label) in listOf(true to R.string.presence_matches, false to R.string.presence_nobody)) {
            val selected = state.value == enabled
            val tag = if (enabled) "presence-matches" else "presence-nobody"
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag(tag).selectable(
                    selected,
                    enabled = state.editable(),
                    role = Role.RadioButton,
                    onClick = { pick(enabled) }
                ),
                verticalAlignment = Alignment.CenterVertically
            ) {
                RadioButton(selected, onClick = null, enabled = state.editable())
                Text(stringResource(label))
            }
        }
    }
}

@Composable
private fun PresenceStatus(state: PresencePresentation, retry: () -> Unit) {
    Caption(stringResource(R.string.presence_hint))
    if (state.busy) Caption(stringResource(R.string.presence_loading))
    if (state.failed) {
        Text(
            stringResource(R.string.presence_failed),
            Modifier.testTag("presence-error").semantics { liveRegion = LiveRegionMode.Polite }
        )
        TextButton(retry, Modifier.testTag("presence-retry"), enabled = !state.busy) {
            Text(stringResource(R.string.presence_retry))
        }
    }
}
