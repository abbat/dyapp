package com.dyapp

import android.content.Context
import android.util.Log
import uniffi.dyapp.ChatSession
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.loadNetwork as ffiLoadNetwork
import uniffi.dyapp.saveNetworkDraft as ffiSaveNetworkDraft

private const val LOG_TAG = "dyapp"

/** Start-screen state; a diagnostic describes persistence after a committed operation. */
class Shown(
    /** Resolved state, or null when storage itself failed. */
    val opened: Opened?,
    /** Picker candidates. */
    val entries: List<Opened> = emptyList(),
    /** Independent committed-persistence detail, never an uncommitted failure. */
    val durabilityDetail: String? = null
)

/** Thin ownership and presentation bridge over common native operations. */
object RustBridge {
    /**
     * Serializes draft transformations until shared core draft serialization is adopted.
     * Always take this before native admission; a lease holder must never wait for this monitor.
     */
    val draftWrites = Any()

    /** Backup-excluded identity storage; opening a registry does not generate keys. */
    fun slots(context: Context) = Slots(context.noBackupFilesDir)

    /** Read settings under verified identity ownership, off the UI thread. */
    fun loadNetwork(opened: Opened.Ready, file: NetworkFile): NetworkSettings? =
        withKey(opened) { dir, _ -> ffiLoadNetwork(dir, file) }

    /** Save settings under verified identity ownership, off the UI thread. */
    fun saveNetworkDraft(opened: Opened.Ready, draft: NetworkSettings) =
        withKey(opened) { dir, _ -> ffiSaveNetworkDraft(dir, draft) }

    /** Retain actual ownership through the whole action; do not return an escaping native worker. */
    fun <T> withKey(opened: Opened.Ready, action: (String, IdentityKey) -> T): T =
        opened.slot.owner().operation(opened.slot.id, opened.peerId).use { operation ->
            operation.key().use { key -> action(operation.slot().directory, key) }
        }

    /** Bootstrap and serialize chats in Rust; call directly without an outer identity lease. */
    fun <T> withChats(opened: Opened.Ready, action: (ChatSession) -> T): T =
        opened.slot.owner().openChats(
            opened.slot.id,
            opened.peerId,
            PlatformKeyStore.existing(identityAlias(opened.slot.id))
        ).use(action)

    /** Resolve an action and its picker candidates; callers dispatch blocking storage off the UI thread. */
    @Synchronized
    fun show(slots: Slots, action: () -> Opened): Shown = runCatching {
        val opened = action()
        val entries = if (opened == Opened.Picker) slots.list().map { slots.inspect(it.id) } else emptyList()
        val detail = slots.takeDurabilityDetail()
        detail?.let { Log.w(LOG_TAG, "Identity change committed; persistence unconfirmed: $it") }
        Shown(opened, entries, detail)
    }.getOrElse { failure ->
        if (failure !is Exception) throw failure
        Log.e(LOG_TAG, "Identity storage failed", failure)
        Shown(null, durabilityDetail = slots.takeDurabilityDetail())
    }
}
