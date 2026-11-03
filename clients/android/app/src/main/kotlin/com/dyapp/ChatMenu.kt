package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatBlock
import uniffi.dyapp.ChatRevocation

/** What the chat's controls do: the screen wires them to the feed, tests count the calls. */
internal class ChatActions(
    val onSend: () -> Unit,
    val onProfile: () -> Unit,
    val onBack: () -> Unit,
    val onClear: () -> Unit,
    val onBlock: () -> Unit,
)

/** Which confirmation the chat shows; saved, so a rotation keeps it open. */
internal enum class ChatDialog { CLEAR, BLOCK }

private const val CLEAR = "chat-clear"

/** `chat-more` and its menu; the dialogs belong to the screen, which [ChatForm.dialog] opens. */
@Composable
internal fun ChatMoreMenu(form: ChatForm, showBlock: Boolean) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton({ open = true }, Modifier.testTag("chat-more"), enabled = !form.busy) {
            Icon(painterResource(R.drawable.ic_more), stringResource(R.string.chat_more))
        }
        DropdownMenu(open, { open = false }, Modifier.testTag("chat-menu")) {
            DropdownMenuItem(text = { Text(stringResource(R.string.chat_clear)) },
                onClick = { open = false; form.dialog.value = ChatDialog.CLEAR }, Modifier.testTag(CLEAR))
            // A request offers `request-block` on its notice instead, as the desktop does.
            if (showBlock) {
                DropdownMenuItem(text = { Text(stringResource(R.string.chat_block)) },
                    onClick = { open = false; form.dialog.value = ChatDialog.BLOCK }, Modifier.testTag("chat-block"))
            }
        }
    }
}

/** The open confirmation: clearing is local only, blocking also has no undo on Android yet. */
@Composable
internal fun ChatConfirm(form: ChatForm, actions: ChatActions) {
    val cancel = { form.dialog.value = null }
    when (form.dialog.value) {
        ChatDialog.CLEAR -> Confirm(CLEAR, R.string.clear_title, listOf(R.string.clear_text, R.string.clear_later),
            R.string.clear_action, cancel) { cancel(); actions.onClear() }
        ChatDialog.BLOCK -> Confirm("block", R.string.block_title, listOf(R.string.block_text), R.string.chat_block,
            cancel) { cancel(); actions.onBlock() }
        null -> Unit
    }
}

@Composable
@Suppress("LongParameterList") // The two dialogs differ in each of these.
private fun Confirm(
    tag: String, title: Int, texts: List<Int>, action: Int, onCancel: () -> Unit, onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onCancel,
        modifier = Modifier.testTag("$tag-dialog"),
        title = { Text(stringResource(title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (text in texts) Text(stringResource(text))
            }
        },
        confirmButton = {
            TextButton(onConfirm, Modifier.testTag("$tag-confirm")) { Text(stringResource(action)) }
        },
        dismissButton = {
            TextButton(onCancel, Modifier.testTag("$tag-cancel")) { Text(stringResource(R.string.remove_cancel)) }
        },
    )
}

/** Clears the chat; the note replaces the thread's start, a failure says why. */
internal fun CoroutineScope.clearChat(form: ChatForm, feed: ChatFeed, peer: String) {
    form.busy = true
    launch {
        withContext(Dispatchers.IO) { feed.clear(peer) }
            .onSuccess { form.cleared = true; form.status = null }
            .onFailure { form.status = ChatStatus(R.string.chat_clear_failed, it.toString()) }
        form.busy = false
    }
}

/** The last block's outcome on Chats: a pending revocation says why; the block itself holds either way. */
@Composable
internal fun BlockedNote(block: ChatBlock) {
    val tag = "chats-blocked-note"
    val text = stringResource(when (block.revocation) {
        ChatRevocation.PENDING -> R.string.chats_blocked_pending
        ChatRevocation.NOT_NEEDED -> R.string.chats_blocked_not_needed
        ChatRevocation.CONFIRMED -> R.string.chats_blocked_confirmed
    })
    Column(Modifier.padding(16.dp)) {
        if (block.revocation == ChatRevocation.PENDING) {
            ErrorNotice(text, tag, block.detail.orEmpty(), "chats-blocked-details", "copy-chats-blocked-details")
        } else {
            Caption(text, Modifier.testTag(tag).semantics { liveRegion = LiveRegionMode.Polite })
        }
    }
}

/** Blocks the peer and leaves the chat, whose row is gone; a failure stays in the chat and says why. */
internal fun CoroutineScope.blockPeer(form: ChatForm, feed: ChatFeed, peer: String, onBack: () -> Unit) {
    form.busy = true
    launch {
        withContext(Dispatchers.IO) { feed.block(peer) }
            .onSuccess { onBack() }
            .onFailure { form.status = ChatStatus(R.string.chat_block_failed, it.toString()) }
        form.busy = false
    }
}
