package com.dyapp

import uniffi.dyapp.ChatSession
import uniffi.dyapp.IdentityKey
import java.io.File

/** Native identity state survives handle disposal and separate instrumentation invocations. */
internal class RetainedTestPeer private constructor(private val root: File, val ready: Opened.Ready) {
    val peerId: String get() = ready.peerId

    fun <T> withChats(action: (ChatSession) -> T): T = RustBridge.withChats(ready, action)

    fun <T> withKey(action: (String, IdentityKey) -> T): T = RustBridge.withKey(ready, action)

    /** Explicit cleanup; ordinary handle disposal never removes the retained identity. */
    fun remove() = Slots(root).remove(ready.slot.id, peerId)

    companion object {
        fun prepare(root: File): RetainedTestPeer {
            val slots = Slots(root)
            check(slots.list().isEmpty() && slots.open() == Opened.Setup) {
                "Retained peer already exists or needs recovery"
            }
            return RetainedTestPeer(root, slots.create())
        }

        fun reopen(root: File): RetainedTestPeer {
            val ready = Slots(root).open() as? Opened.Ready ?: error("Retained peer is absent or needs recovery")
            return RetainedTestPeer(root, ready)
        }
    }
}
