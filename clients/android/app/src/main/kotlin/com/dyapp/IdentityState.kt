package com.dyapp

import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlot
import uniffi.dyapp.MobileSlotRecovery
import uniffi.dyapp.MobileSlotState
import java.io.File

internal const val MISSING_REASON = "missing"
internal const val INCOMPLETE_REASON = "incomplete"
internal const val DELETING_REASON = "deleting"

/** The core's lifecycle, used directly by presentation. */
typealias SlotState = MobileSlotState

/** An inventoried location; presentation never derives a profile directory from its ID. */
class Slot(
    /** Core slot identifier. */
    val id: String,
    /** Directory supplied by native inventory. */
    val dir: File,
    /** Core lifecycle classification. */
    val state: SlotState,
    /** Owning registry for actual operations; absent only in presentation fixtures. */
    internal val registry: MobileIdentityRegistry? = null
)

/** Preserve the location supplied by native inventory and attach its operation owner. */
internal fun MobileSlot.view(registry: MobileIdentityRegistry) = Slot(id, File(directory), state, registry)

/** Existing identity state projected for the start screen. */
sealed class Opened {
    /** No identity exists; creation still requires an explicit action. */
    object Setup : Opened()

    /** Selection is required. */
    object Picker : Opened()

    /** The core actually reopened this identity. */
    class Ready(
        /** Verified location. */
        val slot: Slot,
        /** Peer ID derived from the opaque key. */
        val peerId: String
    ) : Opened()

    /** Recovery can name a missing pointer target without inventing its location. */
    class Recovery(
        /** Candidate, when native inventory actually contains it. */
        val slot: Slot?,
        /** Stable native reason projected for existing localized text. */
        val reason: String,
        /** Peer ID when the key still opens. */
        val peerId: String?,
        /** Native identifier, including a missing pointer target. */
        val id: String? = slot?.id
    ) : Opened()
}

/** Project a native recovery reason without interpreting files or credentials. */
internal fun recoveryView(
    id: String?,
    reason: MobileSlotRecovery,
    peerId: String?,
    entries: List<Slot>
): Opened.Recovery = Opened.Recovery(
    entries.firstOrNull { it.id == id },
    when (reason) {
        MobileSlotRecovery.MISSING -> MISSING_REASON
        MobileSlotRecovery.UNAVAILABLE -> "unavailable"
        MobileSlotRecovery.INCOMPLETE -> INCOMPLETE_REASON
        MobileSlotRecovery.DELETING -> DELETING_REASON
        MobileSlotRecovery.DAMAGED -> "damaged"
        MobileSlotRecovery.INVALID_POINTER -> "invalidPointer"
    },
    peerId,
    id
)

/** Reinspect the displayed candidate on IO without selecting it or inventing a location. */
internal fun Slots.retry(opened: Opened.Recovery): Opened = opened.slot?.let { inspect(it.id) } ?: open()

/** Reject presentation-only locations before any identity data is accessed. */
internal fun Slot.owner(): MobileIdentityRegistry = checkNotNull(registry) { "Identity location has no registry" }

// Removal refuses the legacy slot and damaged or unreadable ones.
internal fun removable(opened: Opened) = when (opened) {
    is Opened.Ready -> opened.slot.id != Slots.LEGACY
    is Opened.Recovery ->
        opened.slot?.let {
            it.id != Slots.LEGACY &&
                opened.reason in setOf(MISSING_REASON, INCOMPLETE_REASON, DELETING_REASON)
        } ?: false
    else -> false
}
