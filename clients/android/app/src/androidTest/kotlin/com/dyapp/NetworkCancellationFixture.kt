package com.dyapp

import android.os.Looper
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlotKeys
import uniffi.dyapp.MobileSlotLayout
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

private const val NETWORK_STEP_SECONDS = 60L

/** One real key-open call can pause after native identity admission, before any draft write. */
internal class NetworkAdmissionGate : MobileSlotKeys by NativeSlotKeys {
    private val armed = AtomicBoolean()
    private val entered = CompletableFuture<Unit>()
    private val released = CountDownLatch(1)

    override fun openWrapped(slotId: String, wrapped: ByteArray): IdentityKey? {
        check(Looper.myLooper() != Looper.getMainLooper()) { "Network key admission reached UI" }
        if (armed.getAndSet(false)) {
            entered.complete(Unit)
            check(
                released.await(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
            ) { "Network admission fixture was not released" }
        }
        return NativeSlotKeys.openWrapped(slotId, wrapped)
    }

    fun arm() = armed.set(true)
    fun awaitEntered() { entered.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS) }
    fun release() = released.countDown()
}

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
