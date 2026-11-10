package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal class ProfileShareForm {
    var shown by mutableStateOf(false)
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)
    var sentTo by mutableStateOf<String?>(null)

    suspend fun send(feed: ChatFeed, target: String, peer: String) {
        sending = true
        val result = withContext(Dispatchers.IO) { feed.shareProfile(target, peer) }
        sending = false
        completeSubmission(result, target)
    }

    fun completeSubmission(result: Result<uniffi.dyapp.ChatSubmission>, target: String) {
        status = sendStatus(result)
        if (result.isSuccess) {
            sentTo = target
            status = status ?: ChatStatus(R.string.profile_shared, null)
        }
    }
}

/** Leaving cancels UI completion; a native send already in flight may still finish. */
@Composable
internal fun ProfileShareButton(peer: String, feed: ChatFeed, openChat: (String) -> Unit) {
    val form = remember(peer, feed) { ProfileShareForm() }
    val scope = rememberCoroutineScope()
    TextButton({
        form.shown = true
        form.status = null
        form.sentTo = null
    }, Modifier.testTag("view-share")) { Text(stringResource(R.string.profile_share)) }
    if (!form.shown) return
    ProfileShareDialog(form, profileShareTargets(feed.rows, peer), openChat) { target ->
        if (!form.sending) {
            form.sending = true
            scope.launch { form.send(feed, target, peer) }
        }
    }
}

@Composable
internal fun ProfileShareDialog(
    form: ProfileShareForm,
    targets: List<uniffi.dyapp.ChatRow>,
    openChat: (String) -> Unit,
    send: (String) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { if (!form.sending) form.shown = false },
        title = { Text(stringResource(R.string.profile_share)) },
        text = { ProfileShareChoices(form, targets, send) },
        confirmButton = {
            form.sentTo?.let { target ->
                TextButton({
                    form.shown = false
                    openChat(target)
                }) { Text(stringResource(R.string.view_message)) }
            }
        },
        dismissButton = {
            TextButton({ form.shown = false }, enabled = !form.sending) {
                Text(stringResource(R.string.search_cancel))
            }
        },
    )
}

@Composable
private fun ProfileShareChoices(form: ProfileShareForm, targets: List<uniffi.dyapp.ChatRow>, send: (String) -> Unit) {
    val names = rememberContactNames(targets.map { it.peerId }.sorted())
    Column(Modifier.verticalScroll(rememberScrollState())) {
        form.status?.let { Text(stringResource(it.text)) }
        when {
            form.sending -> Text(stringResource(R.string.view_sending))
            form.sentTo != null -> Unit
            targets.isEmpty() -> Text(stringResource(R.string.profile_share_empty))
            else -> targets.forEach { row ->
                TextButton({ send(row.peerId) }, Modifier.testTag("profile-share-${row.peerId}")) {
                    Text(contactLabel(row.peerId, names[row.peerId]))
                }
            }
        }
    }
}
