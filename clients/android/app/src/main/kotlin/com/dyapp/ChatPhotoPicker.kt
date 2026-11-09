package com.dyapp

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts.PickVisualMedia
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal val LocalChatPhotoSender =
    androidx.compose.runtime.staticCompositionLocalOf<((PreparedPhoto) -> ChatStatus?)?> { null }

@Composable
internal fun ChatPhotoPicker(enabled: Boolean) {
    val send = LocalChatPhotoSender.current ?: return
    val context = LocalContext.current
    val state = remember { ChatPhotoDraft() }
    val scope = rememberCoroutineScope()
    val picker = rememberLauncherForActivityResult(PickVisualMedia()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            state.busy = true
            state.status = null
            val result = try {
                withContext(Dispatchers.IO) { runCatching { preparePhoto(readPicked(context, uri)) } }
            } finally {
                state.busy = false
            }
            result.onSuccess { state.photo = it }.onFailure {
                state.status = ChatStatus((it as? PhotoProblem)?.text ?: R.string.chat_send_failed, null)
            }
        }
    }
    TextButton(
        { picker.launch(PickVisualMediaRequest(PickVisualMedia.ImageOnly)) },
        enabled = enabled && !state.busy
    ) { Text(androidx.compose.ui.res.stringResource(R.string.chat_photo_attach)) }
    ChatPhotoDraftDialog(state, send, enabled)
}

internal class ChatPhotoDraft {
    var photo by mutableStateOf<PreparedPhoto?>(null)
    var busy by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)

    suspend fun send(sender: (PreparedPhoto) -> ChatStatus?) {
        val prepared = photo ?: return
        busy = true
        try {
            status = withContext(Dispatchers.IO) {
                runCatching { sender(prepared) }.getOrElse { ChatStatus(R.string.chat_send_failed, it.toString()) }
            }
            if (status == null || status?.text == R.string.chat_queued) photo = null
        } finally {
            busy = false
        }
    }
}

@Composable
private fun ChatPhotoDraftDialog(state: ChatPhotoDraft, send: (PreparedPhoto) -> ChatStatus?, enabled: Boolean) {
    val scope = rememberCoroutineScope()
    if (state.photo == null) {
        state.status?.let { Text(androidx.compose.ui.res.stringResource(it.text)) }
        return
    }
    androidx.compose.material3.AlertDialog(
        onDismissRequest = { if (!state.busy) state.photo = null },
        title = { Text(androidx.compose.ui.res.stringResource(R.string.chat_photo)) },
        text = { ChatPhotoConfirmation(state) },
        confirmButton = {
            TextButton({
                if (!state.busy) {
                    state.busy = true
                    scope.launch { state.send(send) }
                }
            }, enabled = enabled && !state.busy) {
                Text(androidx.compose.ui.res.stringResource(R.string.chat_send))
            }
        },
        dismissButton = {
            TextButton({ state.photo = null }, enabled = !state.busy) {
                Text(androidx.compose.ui.res.stringResource(R.string.search_cancel))
            }
        },
    )
}
