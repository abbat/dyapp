package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveFailure
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus

@Composable
internal fun StickerPackControls(target: StickerPackTarget, manage: () -> Unit) = key(target.owner) {
    val state = remember { StickerPackPresentation() }
    val scope = rememberCoroutineScope()
    val current by rememberUpdatedState(target)
    DisposableEffect(state) { onDispose { state.retire() } }
    LaunchedEffect(target.revision) { state.refresh(current.calls) }
    PackControlsContent(target.name, state) { control, shown ->
        if (scope.isActive) {
            when (control) {
                StickerPackSaveControl.REFRESH -> scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    state.refresh(current.calls)
                }
                StickerPackSaveControl.MANAGE -> if (!state.busy) manage()
                else -> shown?.let {
                    // Admit on the event turn before another captured callback can run.
                    scope.launch(start = CoroutineStart.UNDISPATCHED) { state.change(current.calls, it, control) }
                }
            }
        }
    }
}

@Composable
private fun PackControlsContent(
    name: String,
    state: StickerPackPresentation,
    perform: (StickerPackSaveControl, StickerPackSaveStatus?) -> Unit
) {
    val shown = state.status
    val controls = if (state.readFailed || shown == null) {
        listOf(StickerPackSaveControl.REFRESH, StickerPackSaveControl.MANAGE)
    } else {
        shown.controls
    }
    Column(Modifier.fillMaxWidth().testTag("sticker-pack-controls")) {
        Text(stringResource(packStatusText(shown), name), Modifier.testTag("sticker-pack-caption"))
        if (state.readFailed) Text(stringResource(R.string.sticker_pack_read_failed))
        if (state.actionFailed) Text(stringResource(R.string.sticker_pack_action_failed))
        Text(stringResource(R.string.sticker_pack_warning))
        controls.forEach { control ->
            val action = { perform(control, shown) }
            TextButton(
                action,
                Modifier.fillMaxWidth().testTag("sticker-pack-" + control.name).applicationClick(action),
                enabled = !state.busy
            ) { Text(stringResource(packControlText(control))) }
        }
    }
}

private fun packStatusText(status: StickerPackSaveStatus?): Int = when (status?.state) {
    null -> R.string.sticker_pack_checking
    StickerPackSaveState.NEW -> R.string.sticker_pack_new
    StickerPackSaveState.SAVED -> R.string.sticker_pack_saved
    StickerPackSaveState.WAITING -> R.string.sticker_pack_waiting
    StickerPackSaveState.DOWNLOADING -> R.string.sticker_pack_downloading
    StickerPackSaveState.FAILED -> packFailureText(status.failure)
}

private fun packFailureText(failure: StickerPackSaveFailure?): Int = when (failure) {
    StickerPackSaveFailure.FETCH -> R.string.sticker_pack_fetch
    StickerPackSaveFailure.INVALID -> R.string.sticker_pack_invalid
    StickerPackSaveFailure.QUOTA -> R.string.sticker_pack_quota
    StickerPackSaveFailure.STORAGE -> R.string.sticker_pack_storage
    StickerPackSaveFailure.STICKER_MISSING -> R.string.sticker_pack_missing
    null -> R.string.sticker_pack_read_failed
}

private fun packControlText(control: StickerPackSaveControl): Int = when (control) {
    StickerPackSaveControl.REQUEST -> R.string.sticker_pack_request
    StickerPackSaveControl.RETRY -> R.string.sticker_pack_retry
    StickerPackSaveControl.FRESH -> R.string.sticker_pack_fresh
    StickerPackSaveControl.STOP_WAITING -> R.string.sticker_pack_stop
    StickerPackSaveControl.REFRESH -> R.string.sticker_pack_refresh
    StickerPackSaveControl.MANAGE -> R.string.sticker_pack_manage
}
