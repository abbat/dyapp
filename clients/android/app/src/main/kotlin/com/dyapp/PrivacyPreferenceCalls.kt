package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Per-field storage access; all defaults, inversions and disclosure policy remain in the core. */
internal class PrivacyPreferenceCalls(
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
