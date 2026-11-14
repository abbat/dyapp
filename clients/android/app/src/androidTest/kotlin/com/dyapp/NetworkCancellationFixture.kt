package com.dyapp

import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlotLayout
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/** Real platform credentials and registry, isolated from the app's identities and mail job. */
internal class NetworkCancellationFixture : AutoCloseable {
    private val storage = NativeSlotFixture()
    private val original = storage.slots.create()
    val gate = NetworkAdmissionGate()
    private val registry = MobileIdentityRegistry(storage.root.absolutePath, MobileSlotLayout.ANDROID, gate)
    val opened = Opened.Ready(
        Slot(original.slot.id, original.slot.dir, original.slot.state, registry),
        original.peerId
    )

    /** Exclusive core admission proves the cancelled native operation has actually released its lease. */
    fun awaitAdmissionRelease() {
        val finished = CompletableFuture<Unit>()
        thread(name = "network-admission-release", isDaemon = true) {
            runCatching { registry.select(opened.slot.id).key.use {} }
                .fold({ finished.complete(Unit) }, { finished.completeExceptionally(it) })
        }
        finished.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
    }

    override fun close() {
        gate.release()
        registry.close()
        storage.close()
    }
}
