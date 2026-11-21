package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackImport
import uniffi.dyapp.SavedStickerPackPage

/** This opening owns selection/confirmation only; current pages and all pack policy come from core. */
internal class SavedStickerPresentation {
    var page by mutableStateOf<SavedStickerPackPage?>(null)
        private set
    var busy by mutableStateOf(false)
        private set
    var readFailed by mutableStateOf(false)
        private set
    var operationFailed by mutableStateOf(false)
        private set
    var imported by mutableStateOf<SavedStickerPackImport?>(null)
        private set
    var pending by mutableStateOf<SavedStickerRemoval?>(null)
        private set
    var pickerTicket by mutableStateOf<Any?>(null)
        private set
    var cursor: ByteArray? = null
        private set
    private var active = true
    private var known = false
    val editable get() = active && known && !busy && pending == null && pickerTicket == null

    fun retire() { active = false }

    fun requestFile(): Any? {
        if (!editable) return null
        return Any().also { pickerTicket = it }
    }

    suspend fun importPicked(calls: SavedStickerCalls, ticket: Any, read: (suspend () -> ByteArray)?) {
        if (!active || pickerTicket !== ticket) return
        pickerTicket = null
        if (read == null) return
        busy = true
        operationFailed = false
        imported = null
        try {
            val bytes = withContext(Dispatchers.IO) { runCatching { read() } }
            currentCoroutineContext().ensureActive()
            if (active) {
                bytes.fold(
                    onSuccess = { value -> mutate(calls) { calls.save(value) } },
                    onFailure = { operationFailed = true }
                )
            }
        } finally { busy = false }
    }

    suspend fun refresh(
        calls: SavedStickerCalls,
        after: ByteArray? = cursor,
        expected: SavedStickerPackPage? = page
    ) {
        if (!active || busy) return
        if (pickerTicket != null || pending != null || page !== expected) return
        busy = true
        operationFailed = false
        try {
            accept(withContext(Dispatchers.IO) { runCatching { calls.page(after) } }, after)
        } finally { busy = false }
    }

    fun askRemove(entry: SavedStickerPackEntry) {
        if (editable && page?.entries?.any { it === entry } == true) pending = SavedStickerRemoval(entry)
    }

    fun cancelRemove() { if (!busy) pending = null }

    suspend fun confirmRemove(calls: SavedStickerCalls, confirmation: SavedStickerRemoval) {
        if (!active || busy) return
        if (!known || pending !== confirmation) return
        busy = true
        operationFailed = false
        imported = null
        try {
            mutate(calls) {
                calls.remove(confirmation.entry.packId)
                null
            }
        } finally { busy = false }
    }

    private suspend fun mutate(calls: SavedStickerCalls, action: suspend () -> SavedStickerPackImport?) {
        val result = withContext(Dispatchers.IO) { runCatching { action() } }
        currentCoroutineContext().ensureActive()
        if (!active) return
        pending = null
        imported = result.getOrNull()
        operationFailed = result.isFailure
        // Either operation can fail after publication/unlink. Reconcile without repeating it.
        accept(withContext(Dispatchers.IO) { runCatching { calls.page(null) } }, null)
    }

    private suspend fun accept(result: Result<SavedStickerPackPage>, after: ByteArray?) {
        currentCoroutineContext().ensureActive()
        if (!active) return
        known = result.isSuccess
        readFailed = result.isFailure
        result.onSuccess {
            page = it
            cursor = after
        }
    }
}
