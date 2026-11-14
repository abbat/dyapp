package com.dyapp

import android.os.Looper
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileSlotKeys
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

internal const val NETWORK_STEP_SECONDS = 60L

/** One real key-open call can pause after native identity admission, before any draft write. */
internal class NetworkAdmissionGate : MobileSlotKeys by NativeSlotKeys {
    private val armed = AtomicBoolean()
    private val entered = CompletableFuture<Unit>()
    private val opened = CompletableFuture<Unit>()
    private val released = CountDownLatch(1)

    override fun openWrapped(slotId: String, wrapped: ByteArray): IdentityKey? {
        check(Looper.myLooper() != Looper.getMainLooper()) { "Network key admission reached UI" }
        if (!armed.getAndSet(false)) return NativeSlotKeys.openWrapped(slotId, wrapped)
        entered.complete(Unit)
        return runCatching {
            check(
                released.await(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
            ) { "Network admission fixture was not released" }
            NativeSlotKeys.openWrapped(slotId, wrapped)
        }.onSuccess { opened.complete(Unit) }.onFailure { opened.completeExceptionally(it) }.getOrThrow()
    }

    fun arm() = armed.set(true)
    fun awaitEntered() { entered.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS) }
    fun awaitOpened() { opened.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS) }
    fun release() = released.countDown()
}
