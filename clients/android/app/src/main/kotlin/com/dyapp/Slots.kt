package com.dyapp

import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.MobileSlotResolution
import uniffi.dyapp.MobileSlotSelection
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
        return selection.key.use { Opened.Ready(selection.slot.view(registry), it.peerId()) }
    }

    private fun project(resolution: MobileSlotResolution): Opened = when (resolution) {
        MobileSlotResolution.Setup -> Opened.Setup
        is MobileSlotResolution.Picker -> Opened.Picker
        is MobileSlotResolution.Ready -> resolution.key.use {
            Opened.Ready(
                resolution.slot.view(registry),
                it.peerId()
            )
        }
        is MobileSlotResolution.Recovery -> recoveryView(resolution.id, resolution.reason, resolution.peerId, list())
    }

    /** Layout identifier shared with the native registry and credential callback. */
    companion object {
        /** Virtual identity retained at the installation root. */
        const val LEGACY = "legacy-primary"
    }
}
