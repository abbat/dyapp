package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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

internal const val PROFILE_SHARE_OPEN_CHAT_TAG = "profile-share-open-chat"

internal class ProfileShareForm {
    private var visible by mutableStateOf(false)
    var opening by mutableStateOf(Any())
        private set
    var shown: Boolean
        get() = visible
        set(value) {
            if (value && !visible) {
                opening = Any()
                sending = false
                status = null
                sentTo = null
            }
            visible = value
        }
    var sending by mutableStateOf(false)
    var status by mutableStateOf<ChatStatus?>(null)
    var sentTo by mutableStateOf<String?>(null)

    private fun current(expected: Any) = shown && opening === expected

    fun admit(expected: Any): Boolean {
        if (!current(expected) || sending || sentTo != null) return false
        sending = true
        status = null
        return true
    }

    fun cancel(expected: Any) {
        if (current(expected) && !sending) shown = false
    }

    fun navigate(expected: Any, target: String, openChat: (String) -> Unit) {
        if (!current(expected) || sending || sentTo != target) return
        shown = false
        openChat(target)
    }

    suspend fun send(feed: ChatFeed, target: String, peer: String, expected: Any) {
        if (!current(expected) || !sending) return
        val result = try {
            withContext(Dispatchers.IO) { feed.shareProfile(target, peer) }
        } finally {
            if (current(expected)) sending = false
        }
        completeSubmission(result, target, expected)
    }

    fun completeSubmission(result: Result<uniffi.dyapp.ChatSubmission>, target: String, expected: Any) {
        if (!current(expected)) return
        sending = false
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
    TextButton({ form.shown = true }, Modifier.testTag("view-share")) { Text(stringResource(R.string.profile_share)) }
    if (!form.shown) return
    val opening = form.opening
    ProfileShareDialog(form, profileShareTargets(feed.rows, peer), openChat) { target ->
        scope.launch { form.send(feed, target, peer, opening) }
    }
}

@Composable
internal fun ProfileShareDialog(
    form: ProfileShareForm,
    targets: List<uniffi.dyapp.ChatRow>,
    openChat: (String) -> Unit,
    send: (String) -> Unit,
) {
    val opening = form.opening
    val cancel = { form.cancel(opening) }
    DisposableEffect(form, opening) {
        onDispose { if (form.opening === opening) form.shown = false }
    }
    AlertDialog(
        onDismissRequest = cancel,
        title = { Text(stringResource(R.string.profile_share)) },
        text = { ProfileShareChoices(form, targets) { if (form.admit(opening)) send(it) } },
        confirmButton = {
            form.sentTo?.let { target ->
                val navigate = { form.navigate(opening, target, openChat) }
                TextButton(
                    navigate,
                    Modifier.testTag(PROFILE_SHARE_OPEN_CHAT_TAG).applicationClick(navigate)
                ) { Text(stringResource(R.string.view_message)) }
            }
        },
        dismissButton = {
            TextButton(cancel, Modifier.applicationClick(cancel), enabled = !form.sending) {
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
                val choose = { send(row.peerId) }
                TextButton(choose, Modifier.testTag("profile-share-${row.peerId}").applicationClick(choose)) {
                    Text(contactLabel(row.peerId, names[row.peerId]))
                }
            }
        }
    }
}
