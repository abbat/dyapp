package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/** Retained presentation snapshot; native registry calls remain the removal/selection authority. */
internal class IdentityCollectionState {
    var snapshot by mutableStateOf<Shown?>(null)
        private set
    var removing by mutableStateOf<Opened?>(null)
        private set
    var failed by mutableStateOf(false)
        private set
    var busy by mutableStateOf(false)
        private set
    var present = true
        private set

    suspend fun refresh(read: () -> Shown) {
        if (!present || busy) return
        busy = true
        val result = withContext(Dispatchers.IO) { runCatching(read) }
        currentCoroutineContext().ensureActive()
        if (!present) return
        busy = false
        val loaded = result.getOrNull()?.takeIf { it.opened != null }
        failed = loaded == null
        if (loaded != null) snapshot = loaded
    }

    fun choose(entry: Opened) {
        if (present && !busy) removing = entry
    }

    fun dismiss() {
        if (present) removing = null
    }

    fun retire() { present = false }
}
