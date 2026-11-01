package com.dyapp

import android.content.Context
import android.util.Log
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.newChatStateKey
import uniffi.dyapp.loadNetwork as ffiLoadNetwork
import uniffi.dyapp.saveNetworkDraft as ffiSaveNetworkDraft

/** What the start screen shows; [opened] is null when the storage itself fails. */
class Shown(
    /** Active identity state, or null when storage failed. */
    val opened: Opened?,
    /** Candidate identities for the picker, including recovery states. */
    val entries: List<Opened> = emptyList()
)

/** Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings. */
object RustBridge {
    /**
     * Identity slots under `noBackupFilesDir`, which backups skip: their wrapping keys never
     * leave this device, so a restored blob could not be opened anyway.
     */
    fun slots(context: Context) = Slots(context.noBackupFilesDir, PlatformKeyStore)

    /** Read this ready identity's draft or active network settings off the UI thread. */
    fun loadNetwork(opened: Opened.Ready, file: NetworkFile): NetworkSettings? =
        ffiLoadNetwork(opened.slot.dir.absolutePath, file)

    /** Persist draft edits beside this ready identity's key off the UI thread. */
    fun saveNetworkDraft(opened: Opened.Ready, draft: NetworkSettings) =
        ffiSaveNetworkDraft(opened.slot.dir.absolutePath, draft)

    /** Create a checker for this ready slot; run and activate off the UI thread. */
    fun connectionCheck(opened: Opened.Ready) = ConnectionCheck(opened.slot.dir.absolutePath)

    /** Runs [action] on this ready slot's folder and reopened key; a changed Peer ID refuses it. */
    fun <T> withKey(opened: Opened.Ready, action: (String, IdentityKey) -> T): T {
        val slot = opened.slot
        return IdentityKey.unwrap(PlatformKeyStore(slot.alias), slot.keyFile.readBytes()).use { key ->
            check(key.peerId() == opened.peerId) { "The identity key changed" }
            action(slot.dir.absolutePath, key)
        }
    }

    // Not this object's monitor: show() runs on the UI thread and must not wait for a network receive.
    private val chatsLock = Any()

    /**
     * Runs [action] on this ready slot's chats off the UI thread; see [chatKeys]. One session at a
     * time: two would advance the same saved MLS state and the later one would drop the other's commit.
     */
    fun <T> withChats(opened: Opened.Ready, action: (ChatSession) -> T): T = synchronized(chatsLock) {
        withKey(opened) { dir, key ->
            val store = PlatformKeyStore(opened.slot.alias)
            val (deviceKey, stateKey) = chatKeys(opened.slot, store)
            IdentityKey.unwrap(store, deviceKey).use { device ->
                ChatSession.open(dir, key, device, store, stateKey).use(action)
            }
        }
    }

    /** The slot's wrapped device and state keys, made once; Rust refuses a replaced one. */
    @Synchronized
    private fun chatKeys(slot: Slot, store: PlatformKeyStore): Pair<ByteArray, ByteArray> {
        if (!slot.deviceKeyFile.exists()) {
            writeAtomic(slot.deviceKeyFile, IdentityKey.generate().use { it.wrap(store) })
        }
        if (!slot.chatStateKeyFile.exists()) writeAtomic(slot.chatStateKeyFile, newChatStateKey(store))
        return slot.deviceKeyFile.readBytes() to slot.chatStateKeyFile.readBytes()
    }

    // ponytail: Keystore and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /**
     * Runs [action] and returns the screen for its result. A fresh install shows the identity
     * step and creates nothing; the picker lists every slot, opened or with its recovery reason.
     */
    @Synchronized
    fun show(slots: Slots, action: () -> Opened): Shown = runCatching {
        when (val opened = action()) {
            Opened.Picker -> Shown(opened, slots.list().map { openSlot(it, slots::load) })
            else -> Shown(opened)
        }
    }.getOrElse { failure ->
        if (failure !is Exception) throw failure
        Log.e("dyapp", "Identity storage failed", failure)
        Shown(null)
    }
}
