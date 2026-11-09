package com.dyapp

import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState

/** Clear, block and mute actions available for the current chat state. */
@Composable
internal fun ChatStateMenu(form: ChatForm, history: ChatHistory?, onMute: () -> Unit, dismiss: () -> Unit) {
    DropdownMenuItem(text = { Text(stringResource(R.string.chat_clear)) },
        onClick = { dismiss(); form.dialog.value = ChatDialog.CLEAR }, Modifier.testTag("chat-clear"))
    // A request offers `request-block` on its notice instead, as the desktop does.
    if (history?.state != ChatState.REQUEST_RECEIVED) {
        DropdownMenuItem(text = { Text(stringResource(R.string.chat_block)) },
            onClick = { dismiss(); form.dialog.value = ChatDialog.BLOCK }, Modifier.testTag("chat-block"))
    }
    MuteMenuItem(history) { dismiss(); onMute() }
}

@Composable
private fun MuteMenuItem(history: ChatHistory?, onMute: () -> Unit) {
    // As on desktop, a peer with no chat yet has nothing to mute.
    if (history != null && history.state != ChatState.NONE) {
        val label = if (history.muted) R.string.chat_unmute else R.string.chat_mute
        DropdownMenuItem(text = { Text(stringResource(label)) },
            onClick = onMute, Modifier.testTag("chat-mute"))
    }
}
