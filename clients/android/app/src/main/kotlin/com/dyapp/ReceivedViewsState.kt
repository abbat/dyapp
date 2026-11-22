package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import uniffi.dyapp.ReceivedProfileView

/** Read-only presentation for one owner/opening; native code owns dates, ordering and moderation. */
internal class ReceivedViewsState {
    var rows by mutableStateOf<List<ReceivedProfileView>?>(null)
        private set
    var busy by mutableStateOf(false)
        private set
    var failed by mutableStateOf(false)
        private set
    private var present = true

    fun retire() {
        present = false
    }

    fun admits(peer: String): Boolean = present && !busy && rows?.any { it.peerId == peer } == true

    suspend fun load(read: suspend () -> List<ReceivedProfileView>) {
        if (!present || busy) return
        busy = true
        val result = runCatching { read() }
        currentCoroutineContext().ensureActive()
        if (present) {
            result.onSuccess { rows = it }
            failed = result.isFailure
            busy = false
        }
    }
}
