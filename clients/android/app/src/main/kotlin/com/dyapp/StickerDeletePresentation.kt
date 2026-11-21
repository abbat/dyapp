package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatSubmission

/** Confirmation lifetime only; exact ownership, selection and deletion admission stay in the core. */
internal class StickerDeletePresentation {
    var pending by mutableStateOf<Any?>(null)
        private set
    var busy by mutableStateOf(false)
        private set
    var failed by mutableStateOf(false)
        private set
    var submitted by mutableStateOf<ChatSubmission?>(null)
        private set
    private var active = true
    val editable: Boolean get() = active && !busy && submitted == null

    fun retire() {
        active = false
        pending = null
    }

    fun ask() {
        if (editable && pending == null) {
            failed = false
            pending = Any()
        }
    }

    fun cancel(token: Any) {
        if (active && pending === token) pending = null
    }

    suspend fun confirm(token: Any, delete: () -> ChatSubmission) {
        if (!editable || pending !== token) return
        busy = true
        failed = false
        try {
            val result = withContext(Dispatchers.IO) { runCatching(delete) }
            currentCoroutineContext().ensureActive()
            if (active) {
                result.onSuccess {
                    submitted = it
                    pending = null
                }.onFailure { failed = true }
            }
        } finally { busy = false }
    }
}
