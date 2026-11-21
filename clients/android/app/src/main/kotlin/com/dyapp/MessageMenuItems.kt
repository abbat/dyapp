package com.dyapp

import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatMessage

/** Own edit/delete controls and the native-backed forward action for a held text. */
@Composable
internal fun MessageMenuItems(message: ChatMessage, form: ChatForm, writable: Boolean, dismiss: () -> Unit) {
    val context = LocalChatForward.current
    if (context == null || message.author == context.ownerPeerId) {
        OwnMessageMenuItems(message, form, writable, dismiss)
    }
    ChatForwardMenuItem(message, form, dismiss)
}

@Composable
private fun OwnMessageMenuItems(message: ChatMessage, form: ChatForm, writable: Boolean, dismiss: () -> Unit) {
    if (writable) {
        DropdownMenuItem(
            text = { Text(stringResource(R.string.chat_edit)) },
            onClick = {
                dismiss()
                form.edit(message)
            },
            Modifier.testTag("msg-edit"),
            enabled = !form.sending
        )
    }
    DropdownMenuItem(
        text = { Text(stringResource(R.string.chat_delete)) },
        onClick = {
            dismiss()
            form.deleting.value = messageKey(message)
        },
        Modifier.testTag("msg-delete"),
        enabled = !form.busy
    )
}
