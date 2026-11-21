package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.referentialEqualityPolicy
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.ChatSubmission

internal class StickerPickerView(val query: StickerPickerQuery, val data: StickerPickerData?)

/** Presentation history only: core owns pack validation, page bounds, recency and send commitment. */
internal class StickerPickerPresentation {
    var view by mutableStateOf<StickerPickerView?>(null, referentialEqualityPolicy())
        private set
    var busy by mutableStateOf(false)
        private set
    var sendFailed by mutableStateOf(false)
        private set
    var submitted by mutableStateOf<ChatSubmission?>(null)
        private set
    private var active = true
    private var query: StickerPickerQuery = StickerPickerQuery.Packs()
    private var history = emptyList<StickerPickerQuery>()
    val hasPrevious: Boolean get() = history.isNotEmpty()

    fun retire() { active = false }

    private fun admits(shown: StickerPickerView?): Boolean =
        active && !busy && submitted == null && view === shown

    suspend fun refresh(calls: StickerPickerCalls) {
        if (admits(view)) load(calls, query)
    }

    suspend fun navigate(calls: StickerPickerCalls, next: StickerPickerQuery, shown: StickerPickerView) {
        if (!admits(shown) || shown.data == null) return
        history = history + query
        load(calls, next)
    }

    suspend fun back(calls: StickerPickerCalls, shown: StickerPickerView) {
        if (!admits(shown) || history.isEmpty()) return
        val previous = history.last()
        history = history.dropLast(1)
        load(calls, previous)
    }

    suspend fun send(calls: StickerPickerCalls, shown: StickerPickerView, choice: StickerPickerChoice) {
        val choices = shown.data as? StickerPickerData.Choices ?: return
        if (!admits(shown) || choices.entries.none { it === choice }) return
        busy = true
        sendFailed = false
        try {
            val result = withContext(Dispatchers.IO) { runCatching { calls.send(choice) } }
            currentCoroutineContext().ensureActive()
            if (active) result.onSuccess { submitted = it }.onFailure { sendFailed = true }
        } finally { busy = false }
    }

    private suspend fun load(calls: StickerPickerCalls, next: StickerPickerQuery) {
        busy = true
        sendFailed = false
        query = next
        try {
            val result = withContext(Dispatchers.IO) { runCatching { calls.read(next) } }
            currentCoroutineContext().ensureActive()
            if (active) view = StickerPickerView(next, result.getOrNull())
        } finally { busy = false }
    }
}
