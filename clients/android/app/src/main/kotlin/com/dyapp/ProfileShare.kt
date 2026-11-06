package com.dyapp

import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

/** Sharing never creates a message request or sends someone's profile back to them. */
internal fun profileShareTargets(rows: List<ChatRow>, sharedPeer: String): List<ChatRow> =
    rows.filter { it.state == ChatState.OPEN && it.peerId != sharedPeer }
