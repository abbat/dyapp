package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.key

/** A reopened identity is a new owner even when its slot and Peer ID look unchanged. */
@Composable
internal fun rememberOwnedChatForm(owner: Opened.Ready, peer: String): ChatForm = key(owner, peer) {
    rememberChatForm()
}
