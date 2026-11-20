package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Reads local IDs only; a row's explicit Open route owns any later profile network fetch. */
internal class SavedProfilesCalls(val read: suspend () -> List<String>, val remove: suspend (String) -> Unit) {
    suspend fun refresh(removing: String? = null): Result<List<String>> = withContext(Dispatchers.IO) {
        runCatching {
            if (removing != null) remove(removing)
            read()
        }
    }
}

internal fun savedProfilesCalls(opened: Opened.Ready) = SavedProfilesCalls(
    read = { RustBridge.withChats(opened) { it.favoritePeers() } },
    remove = { peer -> RustBridge.withChats(opened) { it.setFavorite(peer, false) } },
)
