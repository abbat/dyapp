package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** One foreground opening owns its platform continuation; no update or chat preference is written. */
internal class UpdatePermissionOpening {
    var pending by mutableStateOf(false)
        private set
    var asked = false
        private set
    var active = true
        private set

    fun begin(requestPermission: Boolean, launch: () -> Unit) {
        if (!active || pending) return
        pending = true
        asked = asked || requestPermission
        runCatching(launch).onFailure { pending = false }.getOrThrow()
    }

    fun complete(read: () -> Unit) {
        if (!active || !pending) return
        pending = false
        read()
    }

    fun retire() {
        active = false
    }
}
