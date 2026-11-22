package com.dyapp

private const val SHORT_PEER_ID = 6

/** Compact presentation label only; complete identifiers remain the action targets. */
internal fun shortPeerId(peerId: String) = "@" + peerId.takeLast(SHORT_PEER_ID)
