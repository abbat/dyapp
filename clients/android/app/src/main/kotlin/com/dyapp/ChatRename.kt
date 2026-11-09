package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** `chat-rename-dialog`: the name only this device shows for the peer; an empty one removes it. */
@Composable
internal fun RenameDialog(form: ChatForm, onSave: () -> Unit) {
    val cancel = { form.renaming.value = null; form.renameError = null }
    AlertDialog(
        onDismissRequest = cancel,
        modifier = Modifier.testTag("chat-rename-dialog"),
        title = { Text(stringResource(R.string.rename)) },
        text = { RenameFields(form) },
        confirmButton = {
            TextButton(onSave, Modifier.testTag("chat-rename-save"), enabled = !form.busy) {
                Text(stringResource(R.string.rename_save))
            }
        },
        dismissButton = {
            TextButton(cancel, Modifier.testTag("chat-rename-cancel")) { Text(stringResource(R.string.remove_cancel)) }
        },
    )
}

@Composable
private fun RenameFields(form: ChatForm) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(form.renaming.value.orEmpty(), { form.renaming.value = it },
            Modifier.testTag("chat-rename-input"), label = { Text(stringResource(R.string.rename_label)) },
            singleLine = true, enabled = !form.busy)
        Caption(stringResource(R.string.rename_hint))
        form.renameError?.let { error ->
            Text(stringResource(error.text, error.details.orEmpty()),
                Modifier.testTag("chat-rename-error").semantics { liveRegion = LiveRegionMode.Polite },
                color = MaterialTheme.colorScheme.error)
        }
    }
}

/** Saves the typed name and closes the dialog, so every row and header reads it; a failure says why. */
internal fun CoroutineScope.renamePeer(form: ChatForm, feed: ChatFeed, peer: String, photos: PeerPhotos?) {
    val name = form.renaming.value ?: return
    form.busy = true
    launch {
        withContext(Dispatchers.IO) { feed.rename(peer, name) }
            .onSuccess {
                form.renaming.value = null
                form.renameError = null
                photos?.reread()
            }
            .onFailure {
                val invalid = "invalidName" in it.toString()
                form.renameError = if (invalid) {
                    ChatStatus(R.string.rename_invalid, null)
                } else {
                    ChatStatus(R.string.rename_failed, it.toString())
                }
            }
        form.busy = false
    }
}
