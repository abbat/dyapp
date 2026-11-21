package com.dyapp

/** Owner includes the actual identity opening, peer and full immutable received-message key. */
internal class StickerPackTarget(
    val owner: Any,
    val name: String,
    val calls: StickerPackCalls,
    val revision: Int = 0,
)
