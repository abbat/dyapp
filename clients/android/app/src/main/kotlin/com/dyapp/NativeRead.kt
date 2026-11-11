package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/**
 * Reads an owned value off UI and drops late results when its screen/identity leaves.
 * Use only for values: native handles require explicit cleanup even after cancellation.
 * The outer key resets remembered data before a replacement owner's first frame.
 */
@Composable
internal fun <T> rememberNativeRead(vararg owners: Any?, read: () -> T): State<Result<T>?> = key(owners.toList()) {
    produceState<Result<T>?>(null) {
        val result = withContext(Dispatchers.IO) { runCatching(read) }
        currentCoroutineContext().ensureActive()
        value = result
    }
}
