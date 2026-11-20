package com.dyapp

import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlot
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.MobileSlotResolution
import uniffi.dyapp.MobileSlotSelection
import uniffi.dyapp.identityRemovalAllowed
import java.io.File

/** Presentation adapter over the common native registry; lifecycle policy belongs to Rust. */
class Slots(root: File) {
    private val registry = MobileIdentityRegistry(root.absolutePath, MobileSlotLayout.ANDROID, NativeSlotKeys)
    private var durabilityDetail: String? = null

    /** Inventoried candidates, including interrupted operations. */
    fun list(): List<Slot> = registry.list().map { it.view(registry) }

    /** Inspect one candidate without changing selection. */
    fun inspect(id: String): Opened = project(registry.inspect(id))

    /** Resolve saved state without creating or replacing an identity. */
    fun open(): Opened = project(registry.resolve())

    /** Persist only a verified ready selection; recovery is presentation navigation. */
    fun choose(id: String): Opened {
        val opened = inspect(id)
        return if (opened is Opened.Ready) selected(registry.select(id)) else opened
    }

    /** Explicitly create and verify a fresh identity through the core. */
    fun create(): Opened.Ready = selected(registry.create())

    /** Remove the explicitly confirmed identity through core ownership and cleanup checks. */
    fun remove(id: String, peerId: String?) {
        durabilityDetail = registry.remove(id, peerId)
    }

    /** A committed operation's independent persistence diagnostic, never a retryable failure. */
    internal fun takeDurabilityDetail(): String? = durabilityDetail.also { durabilityDetail = null }

    private fun selected(selection: MobileSlotSelection): Opened.Ready {
        durabilityDetail = selection.durabilityDetail
        return selection.key.use { projectReady(registry, selection.slot, it) }
    }

    private fun project(resolution: MobileSlotResolution): Opened = when (resolution) {
        MobileSlotResolution.Setup -> Opened.Setup
        is MobileSlotResolution.Picker -> Opened.Picker
        is MobileSlotResolution.Ready -> resolution.key.use { projectReady(registry, resolution.slot, it) }
        is MobileSlotResolution.Recovery -> projectRecovery(resolution, list())
    }

    /** Layout identifier shared with the native registry and credential callback. */
    companion object {
        /** Virtual identity retained at the installation root. */
        const val LEGACY = "legacy-primary"
    }
}

private fun projectReady(registry: MobileIdentityRegistry, slot: MobileSlot, key: IdentityKey): Opened.Ready =
    Opened.Ready(slot.view(registry), key.peerId()).also {
        it.removalOffered = identityRemovalAllowed(slot.id, null, true)
    }

private fun projectRecovery(resolution: MobileSlotResolution.Recovery, entries: List<Slot>): Opened.Recovery =
    recoveryView(resolution.id, resolution.reason, resolution.peerId, entries).also {
        it.removalOffered = identityRemovalAllowed(resolution.id, resolution.reason, it.slot != null)
    }
