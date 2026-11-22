package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext

/** Only the explicit profile page invokes this; cache/list/photo reads remain read-only. */
@Composable
internal fun ProfileViewDisclosure(peer: String, feed: ChatFeed, shown: Boolean) {
    val photos = LocalPeerPhotos.current
    val calls = remember(peer, feed, photos) {
        runCatching {
            photos?.profileViewOpening(peer)?.let { opening ->
                ProfileViewDisclosureCalls(opening::retire) {
                    RustBridge.withChats(feed.opened) { it.submitProfileView(opening) }
                }
            }
        }.getOrNull()
    }
    ProfileViewDisclosureEffect(calls, shown)
}

/** Wait for the actual profile, submit once, and retire unadmitted work when its page disappears. */
@Composable
internal fun ProfileViewDisclosureEffect(calls: ProfileViewDisclosureCalls?, shown: Boolean) {
    val ready by rememberUpdatedState(shown)
    DisposableEffect(calls) { onDispose { calls?.retire?.invoke() } }
    LaunchedEffect(calls) {
        if (calls != null) {
            snapshotFlow { ready }.first { it }
            withContext(Dispatchers.IO) { runCatching(calls.send) }
        }
    }
}
