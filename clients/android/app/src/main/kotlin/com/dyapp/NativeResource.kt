package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.withContext

/**
 * Owns creation and final release on IO, including a factory that returns after cancellation.
 * The action runs in its caller's context, but inherits the IO child's cancellation/ownership.
 * Never return or otherwise retain the handle beyond the action's lifetime.
 */
internal suspend fun <T : AutoCloseable, R> withNativeResource(create: () -> T, action: suspend (T) -> R): R {
    val caller = currentCoroutineContext().minusKey(Job)
    return withContext(Dispatchers.IO) {
        create().use { resource ->
            withContext(caller) { action(resource) }
        }
    }
}
