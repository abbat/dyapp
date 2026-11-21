package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.referentialEqualityPolicy
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatSubmission

internal class ForwardSelection(val source: ChatForwardSource)
internal class ForwardCompletion(val opening: ForwardSelection, val target: String, val result: ChatSubmission)

/** Picker ownership only; current source, destination and committed bytes remain native decisions. */
internal class ChatForwardPresentation {
    var opening by mutableStateOf<ForwardSelection?>(null, referentialEqualityPolicy())
        private set
    var targets by mutableStateOf<List<String>?>(null)
        private set
    var chosen by mutableStateOf<String?>(null)
        private set
    var busy by mutableStateOf(false)
        private set
    var failure by mutableStateOf<Throwable?>(null)
        private set
    var submitted by mutableStateOf<ForwardCompletion?>(null, referentialEqualityPolicy())
        private set
    private var active = true

    fun ask(source: ChatForwardSource) {
        if (active && opening == null && !busy) {
            opening = ForwardSelection(source)
            targets = null
            chosen = null
            failure = null
            submitted = null
        }
    }

    private fun admits(shown: ForwardSelection): Boolean =
        active && opening === shown && !busy && submitted == null

    fun choose(shown: ForwardSelection, target: String) {
        if (admits(shown) && targets?.contains(target) == true) chosen = target
    }

    fun cancel(shown: ForwardSelection) {
        if (admits(shown)) opening = null
    }

    fun retire() {
        active = false
        opening = null
    }

    suspend fun refresh(shown: ForwardSelection, calls: ChatForwardCalls) {
        if (!admits(shown)) return
        busy = true
        targets = null
        chosen = null
        failure = null
        try {
            val result = withContext(Dispatchers.IO) { runCatching { calls.read(shown.source) } }
            currentCoroutineContext().ensureActive()
            if (active && opening === shown) {
                targets = result.getOrNull()
                chosen = targets?.firstOrNull()
                failure = result.exceptionOrNull()
            }
        } finally { busy = false }
    }

    suspend fun send(shown: ForwardSelection, calls: ChatForwardCalls) {
        val target = chosen ?: return
        if (!admits(shown) || targets?.contains(target) != true) return
        busy = true
        failure = null
        try {
            val result = withContext(Dispatchers.IO) { runCatching { calls.send(shown.source, target) } }
            currentCoroutineContext().ensureActive()
            if (active && opening === shown) {
                result.onSuccess { submitted = ForwardCompletion(shown, target, it) }
                    .onFailure { failure = it }
            }
        } finally { busy = false }
    }

    fun complete(shown: ForwardCompletion, open: (String) -> Unit) {
        if (active && submitted === shown && opening === shown.opening) {
            submitted = null
            opening = null
            open(shown.target)
        }
    }
}
