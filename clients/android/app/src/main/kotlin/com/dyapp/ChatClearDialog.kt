package com.dyapp

import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ChatClearTarget
import uniffi.dyapp.ContactNameView

private data class ClearView(
    val opening: ChatClearOpening,
    val result: Result<ChatClearTarget>?,
    val name: ContactNameView?,
    val retry: () -> Unit,
    val busy: Boolean,
)

/** Unknown target never becomes a guessed peer or an enabled destructive action. */
@Composable
internal fun ChatClearDialog(form: ChatForm, name: ContactNameView?, clear: () -> Unit) {
    val calls = LocalChatClearCalls.current
    key(form, calls) {
        var attempt by remember { mutableIntStateOf(0) }
        val result by rememberNativeRead(calls, attempt) {
            checkNotNull(calls) { "Missing owned Clear reader" }.read()
        }
        val opening = remember { ChatClearOpening(form) }
        DisposableEffect(opening) { onDispose { opening.retire() } }
        SideEffect { opening.show(result?.getOrNull()) }
        val retry: () -> Unit = { if (opening.retry()) attempt++ }
        ClearDialogView(ClearView(opening, result, name, retry, form.busy), clear)
    }
}

@Composable
private fun ClearDialogView(view: ClearView, clear: () -> Unit) {
    val shown = view.result?.getOrNull()
    val confirm = { view.opening.confirm(shown, clear) }
    val cancel = view.opening::cancel
    AlertDialog(
        onDismissRequest = cancel,
        modifier = Modifier.testTag("chat-clear-dialog"),
        title = { Text(stringResource(R.string.clear_title)) },
        text = { ClearDialogBody(view) },
        confirmButton = {
            TextButton(
                onClick = confirm,
                Modifier.testTag("chat-clear-confirm").applicationClick(confirm),
                enabled = shown != null && !view.busy
            ) { Text(stringResource(R.string.clear_action)) }
        },
        dismissButton = {
            TextButton(cancel, Modifier.testTag("chat-clear-cancel").applicationClick(cancel)) {
                Text(stringResource(R.string.remove_cancel))
            }
        }
    )
}

@Composable
private fun ClearDialogBody(view: ClearView) {
    Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        val target = view.result?.getOrNull()
        when {
            view.result == null -> {
                Text(stringResource(R.string.view_loading))
            }
            target == null -> {
                Text(stringResource(R.string.chat_clear_target_failed), Modifier.testTag("chat-clear-read-failed"))
                TextButton(view.retry, Modifier.testTag("chat-clear-retry")) {
                    Text(stringResource(R.string.chat_clear_retry))
                }
            }
            else -> {
                ClearTargetText(target, view.name)
                for (text in listOf(R.string.clear_text, R.string.clear_pending, R.string.clear_later)) {
                    Text(stringResource(text))
                }
            }
        }
    }
}

@Composable
private fun ClearTargetText(target: ChatClearTarget, name: ContactNameView?) {
    Text(contactLabel(target.peerId, name), Modifier.testTag("chat-clear-name"))
    Text(stringResource(R.string.search_peer, target.peerId), Modifier.testTag("chat-clear-peer"))
    CopyButton(target.peerId, "copy-chat-clear-peer")
}
