package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Platform calls only; authorization, blocked-target policy and private staging stay in Rust. */
internal class FavoriteCalls(val read: suspend () -> Boolean, val write: suspend (Boolean) -> Unit) {
    suspend fun resolve(target: Boolean?): Result<Boolean> = withContext(Dispatchers.IO) {
        runCatching {
            if (target != null) write(target)
            read()
        }
    }
}

internal fun favoriteCalls(opened: Opened.Ready, peer: String) = FavoriteCalls(
    read = { RustBridge.withChats(opened) { it.favoriteState(peer) } },
    write = { saved -> RustBridge.withChats(opened) { it.setFavorite(peer, saved) } },
)
