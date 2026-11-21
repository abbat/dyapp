package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatPhotoDownload

/** Thin access to the common accepted-contact download policy, with no local network decisions. */
internal class PhotoDownloadCalls(
    val read: suspend () -> ChatPhotoDownload,
    val write: suspend (ChatPhotoDownload) -> Unit,
) {
    suspend fun resolve(target: ChatPhotoDownload?): Result<ChatPhotoDownload> = withContext(Dispatchers.IO) {
        runCatching {
            if (target != null) write(target)
            read()
        }
    }
}

internal fun photoDownloadCalls(active: Opened.Ready) = PhotoDownloadCalls(
    read = { RustBridge.withChats(active) { it.photoDownload() } },
    write = { mode -> RustBridge.withChats(active) { it.setPhotoDownload(mode) } },
)
