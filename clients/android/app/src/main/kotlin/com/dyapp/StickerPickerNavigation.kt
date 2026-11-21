package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** A manager return is tied to its revision; old close callbacks cannot close a new opening. */
internal class StickerPickerNavigation {
    var opening by mutableStateOf<Any?>(null)
        private set
    private var returnFrom: Int? = null
    val waitingForManager: Boolean get() = returnFrom != null

    fun open(enabled: Boolean) {
        if (enabled && opening == null && returnFrom == null) opening = Any()
    }

    fun close(token: Any) {
        if (opening === token) opening = null
    }

    fun manage(token: Any, revision: Int): Boolean {
        if (opening !== token) return false
        returnFrom = revision
        opening = null
        return true
    }

    fun resume(revision: Int, enabled: Boolean) {
        val previous = returnFrom ?: return
        if (previous == revision) return
        returnFrom = null
        if (enabled) opening = Any()
    }
}
