package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.withContext

/** Own the last local snapshot separately from network read-receipt completion. */
@Composable
internal fun rememberChatSnapshot(feed: ChatFeed, peer: String, form: ChatForm): ChatThreadSnapshot? {
    var snapshot by remember(feed, peer) { mutableStateOf<ChatThreadSnapshot?>(null) }
    ChatReadEffects(
        selection = feed to peer to feed.version,
        read = { withContext(Dispatchers.IO) { feed.thread(peer) } },
        onResult = { result ->
            result.onSuccess { shown ->
                if (snapshot == null) form.status = null
                snapshot = shown
            }.onFailure {
                if (snapshot == null) form.status = ChatStatus(R.string.chat_load_failed, it.toString())
            }
        },
        acknowledge = { withContext(Dispatchers.IO) { feed.markRead(peer) } },
    )
    return snapshot
}

/** Refreshes coalesce behind actual reads; network receipts never gate publishing local history. */
@Composable
internal fun ChatReadEffects(
    selection: Pair<Any, Int>,
    read: suspend () -> ChatThreadSnapshot,
    onResult: (Result<ChatThreadSnapshot>) -> Unit,
    acknowledge: suspend () -> Unit,
) {
    val (owner, revision) = selection
    val currentRevision by rememberUpdatedState(revision)
    val currentRead by rememberUpdatedState(read)
    val currentResult by rememberUpdatedState(onResult)
    val currentAcknowledgement by rememberUpdatedState(acknowledge)
    var completed by remember(owner) { mutableIntStateOf(0) }
    LaunchedEffect(owner) {
        snapshotFlow { currentRevision }.collect {
            val result = runCatching { currentRead() }
            // Blocking native work can finish after cancellation; it cannot publish to a new owner.
            currentCoroutineContext().ensureActive()
            currentResult(result)
            if (result.isSuccess) completed += 1
        }
    }
    LaunchedEffect(owner) {
        snapshotFlow { completed }.collect { count ->
            if (count > 0) currentAcknowledgement()
        }
    }
}
