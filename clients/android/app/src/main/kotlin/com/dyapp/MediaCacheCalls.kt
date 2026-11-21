package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatMediaCacheEdit
import uniffi.dyapp.ChatMediaCacheResult
import uniffi.dyapp.ChatMediaCacheSettings

/** Native calls own parsing, current-state edits, eviction and the confirmation decision. */
internal class MediaCacheCalls(
    val read: suspend () -> ChatMediaCacheSettings,
    val change: suspend (ChatMediaCacheEdit, Boolean) -> ChatMediaCacheResult,
) {
    suspend fun resolve(edit: ChatMediaCacheEdit?, confirmed: Boolean = false) = withContext(Dispatchers.IO) {
        runCatching {
            if (edit == null) ChatMediaCacheResult(read(), false) else change(edit, confirmed)
        }
    }
}

internal fun mediaCacheCalls(active: Opened.Ready) = MediaCacheCalls(
    read = { RustBridge.withChats(active) { it.mediaCacheSettings() } },
    change = { edit, confirmed -> RustBridge.withChats(active) { it.changeMediaCache(edit, confirmed) } },
)
