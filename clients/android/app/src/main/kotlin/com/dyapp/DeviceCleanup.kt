package com.dyapp

import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.dyapp.MobileDeviceDirectory

/** Presentation notification after actual native completion; grants no removal authority. */
internal class DeviceCleanupEvent(val slotId: String, val peerId: String, val durabilityDetail: String?)

/** Retains unobserved completions across activity disposal without losing another slot's event. */
internal object DeviceCleanupEvents {
    private val pending = MutableStateFlow(emptyList<DeviceCleanupEvent>())
    val events = pending.asStateFlow()

    fun completed(opened: Opened.Ready, detail: String?) {
        pending.update { it + DeviceCleanupEvent(opened.slot.id, opened.peerId, detail) }
    }

    fun observed(events: List<DeviceCleanupEvent>) {
        pending.update { queued -> queued.filter { it !in events } }
    }
}

/** Null means no verified authority. A nonnull result retains actual cleanup success or failure. */
internal fun completeRevokedDevice(opened: Opened.Ready): Result<String?>? = runCatching {
    MobileDeviceDirectory(
        opened.slot.owner(),
        opened.slot.id,
        opened.peerId,
        PlatformKeyStore.existing(identityAlias(opened.slot.id))
    ).use { directory ->
        directory.revokedDevice()?.use { confirmed ->
            runCatching { confirmed.complete() }.also { result ->
                result.onFailure { Log.i("dyapp", "Revoked device cleanup failed: ${it.javaClass.simpleName}") }
                DeviceCleanupEvents.completed(opened, result.getOrNull())
            }
        }
    }
}.getOrNull()
