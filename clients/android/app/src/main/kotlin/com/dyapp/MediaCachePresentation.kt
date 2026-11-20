package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.dyapp.ChatMediaCacheEdit
import uniffi.dyapp.ChatMediaCacheException
import uniffi.dyapp.ChatMediaCacheResult
import uniffi.dyapp.ChatMediaCacheSettings

/** Only presentation state; the native result decides whether the operation needs a warning. */
internal class MediaCachePresentation {
    var settings by mutableStateOf<ChatMediaCacheSettings?>(null)
        private set
    var draft by mutableStateOf("")
        private set
    var busy by mutableStateOf(true)
        private set
    var failed by mutableStateOf(false)
        private set
    var invalid by mutableStateOf(false)
        private set
    var pending by mutableStateOf<ChatMediaCacheEdit?>(null)
        private set

    private val locked get() = busy || failed || pending != null
    val editable get() = settings != null && !locked
    val budgetEditable get() = editable && settings?.state?.enabled == true
    val canSave get() = budgetEditable && draft != settings?.limitMib

    fun edit(text: String) {
        if (!budgetEditable) return
        draft = text
        invalid = false
    }

    fun admit(edit: ChatMediaCacheEdit?, confirmed: Boolean): Boolean {
        if (busy || edit != null && failed) return false
        return if (confirmed) {
            edit != null && pending === edit
        } else {
            pending == null && allows(edit)
        }
    }

    // Recheck current button affordances even when a disabled control's old callback was captured.
    private fun allows(edit: ChatMediaCacheEdit?): Boolean = when (edit) {
        null -> true
        is ChatMediaCacheEdit.Retention -> editable && edit.enabled != settings?.state?.enabled
        is ChatMediaCacheEdit.Budget -> canSave && edit.mib == draft
        ChatMediaCacheEdit.Clear -> editable && settings?.state?.usedBytes != 0uL
    }

    fun begin() {
        busy = true
        failed = false
        invalid = false
    }

    fun complete(result: Result<ChatMediaCacheResult>, edit: ChatMediaCacheEdit?) {
        result.fold(
            onSuccess = { accept(it, edit) },
            onFailure = {
                pending = null
                invalid = it is ChatMediaCacheException.InvalidLimit
                failed = !invalid
            }
        )
        busy = false
    }

    private fun accept(result: ChatMediaCacheResult, edit: ChatMediaCacheEdit?) {
        val first = settings == null
        settings = result.settings
        failed = false
        invalid = false
        if (result.confirmationRequired) {
            pending = edit
        } else {
            pending = null
            if (first || edit is ChatMediaCacheEdit.Budget) draft = result.settings.limitMib
        }
    }

    fun cancel() {
        if (!busy) pending = null
    }
}
