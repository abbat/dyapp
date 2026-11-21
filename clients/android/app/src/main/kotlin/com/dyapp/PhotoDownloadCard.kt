package com.dyapp

import androidx.compose.foundation.layout.Arrangement
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
import uniffi.dyapp.ChatPhotoDownload

private val modes = listOf(
    Triple(ChatPhotoDownload.BY_NETWORK, "photo-mode-network", R.string.photos_download_network),
    Triple(ChatPhotoDownload.ALWAYS, "photo-mode-always", R.string.photos_download_always),
    Triple(ChatPhotoDownload.ON_TAP, "photo-mode-tap", R.string.photos_download_tap),
)

private class PhotoDownloadPresentation {
    var value by mutableStateOf<ChatPhotoDownload?>(null)
    var busy by mutableStateOf(true)
    var failed by mutableStateOf(false)

    fun admits(target: ChatPhotoDownload?): Boolean {
        if (busy || failed && target != null) return false
        return target == null || value != null && value != target
    }

    fun editable(): Boolean = value != null && !busy && !failed

    fun complete(result: Result<ChatPhotoDownload>) {
        failed = result.isFailure
        result.onSuccess { value = it }
        busy = false
    }
}

@Composable
internal fun PhotoDownloadCard(active: Opened.Ready) =
    PhotoDownloadPicker(active.slot.id, photoDownloadCalls(active))

/** The core owns metered-network and stranger rules; the card stores only the selected enum. */
@Composable
internal fun PhotoDownloadPicker(owner: Any, calls: PhotoDownloadCalls) = key(owner) {
    val state = remember { PhotoDownloadPresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.complete(backend.resolve(null)) }
    val run = { target: ChatPhotoDownload? ->
        if (state.admits(target)) {
            state.busy = true
            state.failed = false
            scope.launch { state.complete(backend.resolve(target)) }
        }
    }
    FormCard {
        CardTitle(R.string.photos_download_question)
        PhotoDownloadChoices(state, run)
        PhotoDownloadStatus(state) { run(null) }
    }
}

@Composable
private fun PhotoDownloadStatus(state: PhotoDownloadPresentation, retry: () -> Unit) {
    Caption(stringResource(R.string.photos_download_hint))
    if (state.busy) Caption(stringResource(R.string.photos_download_loading))
    if (state.failed) {
        Text(
            stringResource(R.string.photos_download_failed),
            Modifier.testTag("photo-mode-error").semantics { liveRegion = LiveRegionMode.Polite }
        )
        TextButton(retry, Modifier.testTag("photo-mode-retry"), enabled = !state.busy) {
            Text(stringResource(R.string.photos_download_retry))
        }
    }
}

@Composable
private fun PhotoDownloadChoices(state: PhotoDownloadPresentation, pick: (ChatPhotoDownload) -> Unit) {
    Column(Modifier.selectableGroup(), Arrangement.spacedBy(8.dp)) {
        for ((mode, tag, label) in modes) {
            val selected = state.value == mode
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag(tag).selectable(
                    selected = selected,
                    enabled = state.editable(),
                    role = Role.RadioButton,
                    onClick = { pick(mode) }
                ),
                verticalAlignment = Alignment.CenterVertically
            ) {
                RadioButton(selected, onClick = null, enabled = state.editable())
                Text(stringResource(label))
            }
        }
    }
}
