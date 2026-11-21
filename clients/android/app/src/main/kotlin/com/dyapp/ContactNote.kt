package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.dyapp.ContactNoteException

private class NotePresentation {
    var saved by mutableStateOf<String?>(null)
    var draft by mutableStateOf("")
    var busy by mutableStateOf(true)
    var message by mutableStateOf<Int?>(R.string.notes_loading)
    val editable get() = !busy && saved != null
    val canSave get() = editable && draft != saved
    val canClear get() = editable && saved?.isNotEmpty() == true

    fun edit(text: String) {
        if (!editable) return
        draft = text
        message = null
    }

    fun target(clear: Boolean): String? {
        if (!editable) return null
        val target = if (clear) "" else draft
        return target.takeIf { it != saved }
    }

    fun complete(result: Result<String>, target: String?) {
        result.fold(
            onSuccess = {
                saved = it
                draft = it
                message = when {
                    target == null -> null
                    it.isEmpty() -> R.string.notes_cleared
                    else -> R.string.notes_saved
                }
            },
            onFailure = {
                message = when {
                    it is ContactNoteException.InvalidText -> R.string.notes_invalid
                    target == null -> R.string.notes_load_failed
                    else -> R.string.notes_save_failed
                }
            }
        )
        busy = false
    }
}

/** One profile opening owns the private draft; a replacement cancels its old UI continuations. */
@Composable
internal fun ContactNote(owner: Any, calls: ContactNoteCalls) = key(owner) {
    val state = remember { NotePresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.complete(backend.resolve(null), null) }
    val run = { target: String? ->
        if (!state.busy) {
            state.busy = true
            state.message = if (target == null) R.string.notes_loading else R.string.notes_saving
            scope.launch { state.complete(backend.resolve(target), target) }
        }
    }
    FormCard {
        NoteFields(state)
        NoteActions(state, run)
    }
}

@Composable
private fun NoteFields(state: NotePresentation) {
    OutlinedTextField(
        value = state.draft,
        onValueChange = state::edit,
        modifier = Modifier.fillMaxWidth().testTag("view-note-text"),
        label = { Text(stringResource(R.string.notes_title)) },
        enabled = state.editable,
        minLines = 3,
        maxLines = 8,
        isError = state.message == R.string.notes_invalid
    )
    Caption(stringResource(R.string.notes_hint))
    state.message?.let {
        Text(stringResource(it), Modifier.testTag("view-note-status").semantics { liveRegion = LiveRegionMode.Polite })
    }
}

@Composable
private fun NoteActions(state: NotePresentation, run: (String?) -> Unit) {
    if (state.saved == null) {
        TextButton({ run(null) }, Modifier.testTag("view-note-retry"), enabled = !state.busy) {
            Text(stringResource(R.string.notes_retry))
        }
    } else {
        Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(8.dp)) {
            Button(
                { state.target(false)?.let(run) },
                Modifier.weight(1f).testTag("view-note-save"),
                enabled = state.canSave
            ) { Text(stringResource(R.string.notes_save)) }
            TextButton(
                { state.target(true)?.let(run) },
                Modifier.weight(1f).testTag("view-note-clear"),
                enabled = state.canClear
            ) { Text(stringResource(R.string.notes_clear)) }
        }
    }
}
