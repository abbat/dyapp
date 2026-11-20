package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import uniffi.dyapp.FoundProfile

/** The chat and profile navigation retained for one identity. */
internal class ShellPeers(
    chat: MutableState<String?>,
    viewed: MutableState<String?>,
    found: MutableState<FoundProfile?>,
    fromSaved: MutableState<Boolean> = mutableStateOf(false),
) {
    var chat by chat
    var viewed by viewed
    var found by found
    var fromSaved by fromSaved

    fun close() {
        viewed = null
        found = null
    }

    fun showSaved(peer: String) {
        fromSaved = true
        found = null
        viewed = peer
    }

    fun show(profile: FoundProfile) {
        fromSaved = false
        found = profile
        viewed = profile.peerId
    }
}

/** Restore peer IDs, but fetch a verified profile again after recreation. */
@Composable
internal fun rememberShellPeers(identity: String): ShellPeers {
    val chat = rememberSaveable(identity) { mutableStateOf<String?>(null) }
    val viewed = rememberSaveable(identity) { mutableStateOf<String?>(null) }
    // The result already verified; after a recreation only the Peer ID is left and the page fetches.
    val found = remember(identity) { mutableStateOf<FoundProfile?>(null) }
    val fromSaved = rememberSaveable(identity) { mutableStateOf(false) }
    return remember(identity) { ShellPeers(chat, viewed, found, fromSaved) }
}

/** The viewed profile, including return-to-chat and block navigation. */
@Composable
internal fun ShellPeers.Profile(
    opened: Opened.Ready,
    section: StartTarget?,
    feed: ChatFeed,
    open: (StartTarget?) -> Unit,
) {
    val peers = this
    val viewed = peers.viewed ?: return
    val backLabel = if (peers.chat == null && section == StartTarget.SEARCH && !peers.fromSaved) {
        R.string.view_back
    } else {
        R.string.lock_back
    }
    val toChats = { peer: String? ->
        peers.close()
        peers.chat = peer
        // Back from a chat always lands on Chats, as the prototype's back button says.
        open(StartTarget.CHATS)
    }
    // A block leaves for Chats, whose note says the outcome, even when opened from a chat.
    val actions = ProfileActions(backLabel, peers::close, { toChats(viewed) }) { toChats(null) }
    ProfileViewScreen(opened, viewed, peers.found, feed, actions) { toChats(it) }
}
