package com.dyapp

import uniffi.dyapp.ChatBlock
import uniffi.dyapp.ChatLikes
import uniffi.dyapp.ChatState

/** Peer actions share the feed's identity lease and refresh its rows after persisted changes. */
internal fun ChatFeed.likes(peer: String) = RustBridge.withChats(opened) { it.likes(peer) }

/** The verified profiles of [peers]; omit profiles no node returns. */
internal fun ChatFeed.profiles(peers: List<String>) = RustBridge.withChats(opened) { it.profiles(peers) }

/** Sends or withdraws a like, then refreshes local rows. */
internal fun ChatFeed.like(peer: String, liked: Boolean): Result<ChatLikes> =
    runCatching { RustBridge.withChats(opened) { it.like(peer, liked) } }.also { read() }

/** Passes on a peer locally, or undoes that, then refreshes rows. */
internal fun ChatFeed.pass(peer: String, passed: Boolean): Result<ChatLikes> =
    runCatching { RustBridge.withChats(opened) { it.setPassed(peer, passed) } }.also { read() }

/** Shares a core-formatted profile link into an open eligible conversation. Call on IO. */
internal fun ChatFeed.shareProfile(peer: String, sharedPeer: String): ChatStatus? {
    val text = requireNotNull(uniffi.dyapp.profileLinkText(sharedPeer)) { "invalid profile peer" }
    val before = history(peer)
    require(before.state == ChatState.OPEN && peer != sharedPeer) { "not an eligible chat" }
    return send(peer, text, before.messages)
}

/** Clears this device's history for the peer, then refreshes rows. */
internal fun ChatFeed.clear(peer: String): Result<UInt> =
    runCatching { RustBridge.withChats(opened) { it.clearChat(peer) } }.also { read() }

/** Mutes the peer's new-mail notices, or ends the mute. */
internal fun ChatFeed.mute(peer: String, muted: Boolean): Result<Unit> =
    runCatching { RustBridge.withChats(opened) { it.setMuted(peer, muted) } }.also { read() }

/** Sets a device-local contact name; an empty name removes it. */
internal fun ChatFeed.rename(peer: String, name: String): Result<Unit> =
    runCatching { RustBridge.withChats(opened) { it.setContactName(peer, name) } }

/** Blocks the peer and retains the outcome while refreshing rows. */
internal fun ChatFeed.block(peer: String): Result<ChatBlock> =
    runCatching { RustBridge.withChats(opened) { it.block(peer) } }
        .onSuccess { lists.blocked = it }
        .also { read() }
