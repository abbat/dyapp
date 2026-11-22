package com.dyapp

import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileDeviceReadState
import uniffi.dyapp.MobileDeviceRowSource
import uniffi.dyapp.MobileDeviceSnapshot
import uniffi.dyapp.MobileOwnDevice
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Controlled native completions test presentation ownership, not wire/device authorization. */
internal class DeviceDirectoryFixture {
    val other = ByteArray(32) { 2 }
    val snapshot = MobileDeviceSnapshot(
        listOf(MobileOwnDevice(ByteArray(32) { 1 }, true), MobileOwnDevice(other, false)),
        MobileDeviceReadState.CACHED,
        MobileDeviceRowSource.SAVED
    )
    val reads = AtomicReference<(Boolean) -> MobileDeviceSnapshot>({ snapshot })
    val writes = AtomicReference<(ByteArray) -> Unit>({})
    val writeCount = AtomicInteger()
    val closeCount = AtomicInteger()
    val session = DeviceDirectorySession {
        DeviceDirectoryCalls(
            { reads.get().invoke(it) },
            { key ->
                writeCount.incrementAndGet()
                writes.get().invoke(key)
            },
            { closeCount.incrementAndGet() }
        )
    }

    suspend fun load(): DeviceDirectoryListing = withTimeout(5_000) {
        checkNotNull(session.refresh(false)).await().getOrThrow()
    }

    suspend fun close() = withTimeout(5_000) { session.retire().await().getOrThrow() }
}
