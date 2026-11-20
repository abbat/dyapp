package com.dyapp

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.dyapp.ChatMediaCacheEdit

@Composable
internal fun MediaCacheCard(active: Opened.Ready) = MediaCacheControls(active.slot.id, mediaCacheCalls(active))

/** Every opening owns its draft and continuations; no native lease is retained by the dialog. */
@Composable
internal fun MediaCacheControls(owner: Any, calls: MediaCacheCalls) = key(owner) {
    val state = remember { MediaCachePresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.complete(backend.resolve(null), null) }
    val run = { edit: ChatMediaCacheEdit?, confirmed: Boolean ->
        if (state.admit(edit, confirmed)) {
            state.begin()
            scope.launch { state.complete(backend.resolve(edit, confirmed), edit) }
        }
    }
    FormCard {
        CardTitle(R.string.cache_title)
        CacheRetention(state) { run(it, false) }
        CacheBudget(state) { run(ChatMediaCacheEdit.Budget(state.draft), false) }
        CacheActions(state) { run(it, false) }
        CacheStatus(state) { run(null, false) }
    }
    MediaCacheDialog(state) { run(it, true) }
}

@Composable
private fun CacheRetention(state: MediaCachePresentation, run: (ChatMediaCacheEdit) -> Unit) {
    val checked = state.settings?.state?.enabled == true
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("cache-enabled").toggleable(
            checked,
            enabled = state.editable,
            role = Role.Switch,
            onValueChange = { run(ChatMediaCacheEdit.Retention(it)) }
        ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(stringResource(R.string.cache_keep), Modifier.weight(1f))
        Switch(checked, onCheckedChange = null, enabled = state.editable)
    }
}

@Composable
private fun CacheBudget(state: MediaCachePresentation, save: () -> Unit) {
    OutlinedTextField(
        value = state.draft,
        onValueChange = state::edit,
        modifier = Modifier.fillMaxWidth().testTag("cache-limit"),
        enabled = state.budgetEditable,
        label = { Text(stringResource(R.string.cache_limit)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        singleLine = true,
        isError = state.invalid
    )
    state.settings?.let {
        Caption(stringResource(R.string.cache_saved_limit, it.limitMib, it.minMib, it.maxMib))
        Caption(stringResource(R.string.cache_usage, it.state.usedBytes.toString()))
    }
    Button(save, Modifier.testTag("cache-save"), enabled = state.canSave) {
        Text(stringResource(R.string.cache_save))
    }
}

@Composable
private fun CacheActions(state: MediaCachePresentation, run: (ChatMediaCacheEdit) -> Unit) {
    val hasFiles = state.settings?.state?.usedBytes != 0uL
    TextButton(
        { run(ChatMediaCacheEdit.Clear) },
        Modifier.testTag("cache-clear"),
        enabled = state.editable && hasFiles
    ) { Text(stringResource(R.string.cache_clear)) }
    Caption(stringResource(R.string.cache_hint))
}

@Composable
private fun CacheStatus(state: MediaCachePresentation, retry: () -> Unit) {
    if (state.busy) Caption(stringResource(R.string.cache_loading))
    if (state.invalid) {
        Text(
            stringResource(R.string.cache_invalid),
            Modifier.testTag("cache-invalid").semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
    if (state.failed) {
        Text(
            stringResource(R.string.cache_failed),
            Modifier.testTag("cache-error").semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
    val tag = if (state.failed) "cache-retry" else "cache-refresh"
    TextButton(retry, Modifier.testTag(tag), enabled = !state.busy && state.pending == null) {
        Text(stringResource(if (state.failed) R.string.cache_retry else R.string.cache_refresh))
    }
}
