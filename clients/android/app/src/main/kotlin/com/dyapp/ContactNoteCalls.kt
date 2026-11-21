package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Preserve exact strings; validation and private own-device staging belong to the common core. */
internal class ContactNoteCalls(val read: suspend () -> String, val write: suspend (String) -> Unit) {
    suspend fun resolve(target: String?): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            if (target != null) write(target)
            read()
        }
    }
}

internal fun contactNoteCalls(opened: Opened.Ready, peer: String) = ContactNoteCalls(
    read = { RustBridge.withChats(opened) { it.contactNote(peer) } },
    write = { text -> RustBridge.withChats(opened) { it.setContactNote(peer, text) } },
)
