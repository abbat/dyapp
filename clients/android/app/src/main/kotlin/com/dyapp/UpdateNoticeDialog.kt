package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateOffer

/** Renders core data; opening a browser never installs or acknowledges an update. */
@Composable
internal fun UpdateNoticeDialog(
    state: UpdateNoticeUiState,
    later: (MobileUpdateOffer) -> Unit,
    open: (String) -> Unit,
) {
    val offer = state.snapshot?.notice?.takeUnless { it.delivery == MobileUpdateDelivery.STORE } ?: return
    val opening = rememberNoticeOpening(offer, state.pending)
    val acknowledge = { opening.later(later) }
    AlertDialog(
        onDismissRequest = acknowledge,
        modifier = Modifier.testTag("update-notice"),
        title = { Text(stringResource(R.string.settings_updates)) },
        text = { UpdateNoticeBody(state, offer, opening.failed) },
        confirmButton = { UpdateNoticeOpenButton(offer, opening, state.pending, open) },
        dismissButton = {
            TextButton(
                acknowledge,
                Modifier.testTag("update-notice-later").applicationClick(acknowledge),
                enabled = !state.pending,
            ) {
                Text(stringResource(R.string.update_later))
            }
        },
    )
}

private class UpdateNoticeOpening(private val offer: MobileUpdateOffer) {
    var active = true
    var pending = false
    var opened by mutableStateOf(false)
        private set
    var failed by mutableStateOf(false)
        private set

    fun open(action: (String) -> Unit) {
        if (active && !pending && !opened) {
            val result = runCatching { action(offer.releasePage) }
            opened = result.isSuccess
            failed = result.isFailure
        }
    }

    fun later(action: (MobileUpdateOffer) -> Unit) {
        if (active && !pending) action(offer)
    }
}

@Composable
private fun rememberNoticeOpening(offer: MobileUpdateOffer, pending: Boolean): UpdateNoticeOpening {
    val opening = remember(offer) { UpdateNoticeOpening(offer) }
    DisposableEffect(opening) { onDispose { opening.active = false } }
    SideEffect { opening.pending = pending }
    return opening
}

@Composable
private fun UpdateNoticeOpenButton(
    offer: MobileUpdateOffer,
    opening: UpdateNoticeOpening,
    pending: Boolean,
    open: (String) -> Unit,
) {
    val launch = { opening.open(open) }
    TextButton(
        onClick = launch,
        modifier = Modifier.testTag("update-notice-open").applicationClick(launch),
        enabled = !pending && !opening.opened,
    ) {
        val label = if (offer.delivery == MobileUpdateDelivery.BUILD_FROM_SOURCE) {
            R.string.update_open_source
        } else {
            R.string.update_open_release
        }
        Text(stringResource(label))
    }
}

@Composable
private fun UpdateNoticeBody(state: UpdateNoticeUiState, offer: MobileUpdateOffer, openFailed: Boolean) {
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(stringResource(R.string.update_installed, checkNotNull(state.snapshot).installedVersion))
        Text(updateOfferText(offer))
        val status = when {
            state.pending -> R.string.update_notice_saving
            state.failed -> R.string.update_notice_failed
            openFailed -> R.string.update_open_failed
            else -> null
        }
        status?.let {
            Caption(
                stringResource(it),
                Modifier.testTag("update-notice-status").semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
}
