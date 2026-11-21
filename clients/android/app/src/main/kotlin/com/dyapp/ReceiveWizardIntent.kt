package com.dyapp

import androidx.compose.runtime.saveable.listSaver

/** Restored navigation intent, not authority to remove an identity or start native work. */
internal class ReceiveWizardIntent(val id: String = "", val peer: String = "") {
    val active get() = id.isNotEmpty() && peer.isNotEmpty()

    fun matches(opened: Opened?): Boolean {
        if (!active) return false
        return opened is Opened.Ready && opened.slot.id == id && opened.peerId == peer
    }
}

internal val receiveWizardIntentSaver = listSaver<ReceiveWizardIntent, String>(
    save = { listOf(it.id, it.peer) },
    restore = { ReceiveWizardIntent(it.getOrNull(0).orEmpty(), it.getOrNull(1).orEmpty()) }
)
