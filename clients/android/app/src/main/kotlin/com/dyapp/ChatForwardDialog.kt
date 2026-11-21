package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

@Composable
internal fun ChatForwardDialog(state: ChatForwardPresentation, actions: ChatForwardActions) {
    val enabled = !state.busy && state.submitted == null
    AlertDialog(
        onDismissRequest = actions::cancel,
        modifier = Modifier.testTag("chat-forward-dialog"),
        title = { Text(stringResource(R.string.chat_forward_title)) },
        text = { ChatForwardBody(state, actions) },
        confirmButton = {
            ConfirmAction("chat-forward-confirm", R.string.chat_forward, enabled && state.chosen != null, actions::send)
        },
        dismissButton = {
            ConfirmAction("chat-forward-cancel", R.string.remove_cancel, state.canCancel, actions::cancel)
        }
    )
}

@Composable
private fun ChatForwardBody(state: ChatForwardPresentation, actions: ChatForwardActions) {
    Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        ChatForwardErrorNotice(state)
        when {
            state.busy -> {
                Text(stringResource(if (state.targets == null) R.string.view_loading else R.string.view_sending))
            }
            state.targets == null -> {
                if (state.failure == null) Text(stringResource(R.string.view_loading))
                TextButton(actions::refresh, Modifier.testTag("forward-retry")) {
                    Text(stringResource(R.string.search_retry))
                }
            }
            state.targets?.isEmpty() == true -> {
                Text(stringResource(R.string.chat_forward_empty), Modifier.testTag("forward-no-targets"))
            }
            else -> {
                ChatForwardTargets(state, actions)
            }
        }
    }
}

@Composable
private fun ChatForwardErrorNotice(state: ChatForwardPresentation) {
    val error = state.failure ?: return
    val text = if (state.targets == null) R.string.chat_forward_load_failed else R.string.chat_forward_failed
    ErrorNotice(
        stringResource(text),
        "forward-failed",
        error.toString(),
        "forward-details",
        "copy-forward-details"
    )
}
