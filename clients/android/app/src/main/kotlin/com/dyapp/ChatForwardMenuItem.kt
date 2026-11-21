package com.dyapp

import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatMessage

@Composable
internal fun hasForwardAction(message: ChatMessage): Boolean =
    LocalChatForward.current != null && !message.deleted && !message.text.isNullOrEmpty()

@Composable
internal fun ChatForwardMenuItem(message: ChatMessage, form: ChatForm, dismiss: () -> Unit) {
    val context = LocalChatForward.current ?: return
    if (!hasForwardAction(message)) return
    val guarded = rememberConfirmation({ !form.busy }, dismiss) {
        dismiss()
        context.state.ask(
            ChatForwardSource(context.peer, message.groupId.copyOf(), message.author, message.id.copyOf())
        )
    }
    DropdownMenuItem(
        text = { Text(stringResource(R.string.chat_forward)) },
        onClick = guarded.confirm,
        Modifier.testTag("msg-forward").applicationClick(guarded.confirm),
        enabled = guarded.enabled
    )
}
