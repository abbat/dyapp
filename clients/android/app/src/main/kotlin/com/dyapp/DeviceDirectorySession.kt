package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Only presentation admission/lifetime; directory policy and revocation stay in common Rust. */
internal class DeviceDirectorySession(private val create: () -> DeviceDirectoryCalls) {
    private val monitor = Any()
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val current = MutableStateFlow(DeviceDirectoryViewState())
    val state = current.asStateFlow()
    private var native: DeviceDirectoryCalls? = null
    private var closing = false
    private val released = CompletableDeferred<Result<Unit>>()

    fun refresh(live: Boolean): Deferred<Result<DeviceDirectoryListing>>? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired) return@synchronized null
        val result = CompletableDeferred<Result<DeviceDirectoryListing>>()
        current.value = before.copy(
            operation = if (live) DeviceDirectoryOperation.REFRESH else DeviceDirectoryOperation.CACHED,
            confirmation = null, failure = null
        )
        worker.launch {
            val outcome = runCatching { DeviceDirectoryListing(calls().read(live)) }
            synchronized(monitor) {
                current.value = current.value.copy(
                    listing = outcome.getOrNull() ?: current.value.listing,
                    operation = null, failure = outcome.exceptionOrNull()
                )
            }
            result.complete(outcome)
            releaseIfRetired()
        }
        result
    }

    fun request(row: DeviceDirectoryRow): DeviceDisconnectOpening? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired) return@synchronized null
        if (before.confirmation != null || row.current) return@synchronized null
        if (before.listing?.rows?.none { it === row } != false) return@synchronized null
        val opening = DeviceDisconnectOpening(row)
        current.value = before.copy(confirmation = opening)
        opening
    }

    fun dismiss(expected: DeviceDisconnectOpening): Boolean = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired || before.confirmation !== expected) return@synchronized false
        current.value = before.copy(confirmation = null)
        true
    }

    fun confirm(expected: DeviceDisconnectOpening): Deferred<Result<Unit>>? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired || before.confirmation !== expected) return@synchronized null
        val target = expected.target.key()
        val result = CompletableDeferred<Result<Unit>>()
        current.value = before.copy(operation = DeviceDirectoryOperation.REVOKE, failure = null)
        worker.launch {
            val outcome = runCatching { calls().revoke(target) }
            // A cache read must not erase the actual write result or turn its failure into rollback.
            val refreshed = runCatching { DeviceDirectoryListing(calls().read(false)) }
            synchronized(monitor) {
                current.value = current.value.copy(
                    listing = refreshed.getOrNull() ?: current.value.listing,
                    operation = null,
                    confirmation = if (outcome.isSuccess) null else current.value.confirmation,
                    completion = DeviceDisconnectCompletion(expected.target, outcome),
                    failure = refreshed.exceptionOrNull()
                )
            }
            result.complete(outcome)
            releaseIfRetired()
        }
        result
    }

    /** Refuse future presentation actions immediately; an admitted native call finishes first. */
    fun retire(): Deferred<Result<Unit>> {
        synchronized(monitor) {
            current.value = current.value.copy(retired = true, confirmation = null)
        }
        releaseIfRetired()
        return released
    }

    private fun calls(): DeviceDirectoryCalls = native ?: create().also { native = it }

    private fun releaseIfRetired() = synchronized(monitor) {
        if (current.value.retired && !current.value.busy && !closing) {
            closing = true
            worker.launch {
                val outcome = runCatching<Unit> { native?.close?.invoke() }
                synchronized(monitor) {
                    current.value = current.value.copy(failure = current.value.failure ?: outcome.exceptionOrNull())
                }
                released.complete(outcome)
                worker.cancel()
            }
        }
    }
}
