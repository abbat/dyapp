package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Storage access only; the core owns audience, eligibility and throttling. */
internal class PresenceCalls(
    val read: suspend () -> Boolean,
    val write: suspend (Boolean) -> Unit,
) {
    suspend fun resolve(target: Boolean?): Result<Boolean> = withContext(Dispatchers.IO) {
        runCatching {
            if (target != null) write(target)
            read()
        }
    }
}

internal fun presenceCalls(active: Opened.Ready) = PresenceCalls(
    read = { RustBridge.withChats(active) { it.presenceEnabled() } },
    write = { enabled -> RustBridge.withChats(active) { it.setPresenceEnabled(enabled) } },
)
