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
import uniffi.dyapp.MobileSearchRecovery
import uniffi.dyapp.SearchRecoverySaved

internal class SearchRecoveryCalls(
    val confirm: () -> SearchRecoverySaved,
    val retry: () -> SearchRecoverySaved,
    val close: () -> Unit
)

internal fun searchRecoverySession(opened: Opened.Ready) = SearchRecoverySession {
    MobileSearchRecovery(
        opened.slot.owner(),
        opened.slot.id,
        opened.peerId,
        PlatformKeyStore.existing(identityAlias(opened.slot.id))
    ).use { recovery ->
        recovery.prepare()?.let { review ->
            SearchRecoveryCalls(
                { withSearchOutboxChange { review.confirm() } },
                review::retry,
                { try { review.dismiss() } finally { review.close() } }
            )
        }
    }
}

/** Reference identity prevents a callback from admitting work for a replacement review. */
internal class SearchRecoveryOpening

internal data class SearchRecoveryView(
    val opening: SearchRecoveryOpening? = null,
    val busy: Boolean = false,
    val saved: SearchRecoverySaved? = null,
    val failure: Throwable? = null,
    val retired: Boolean = false
)

/** Presentation ownership only; shared core owns eligibility, signatures, versions and delivery. */
internal class SearchRecoverySession(private val create: () -> SearchRecoveryCalls?) {
    private val monitor = Any()
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val current = MutableStateFlow(SearchRecoveryView())
    val state = current.asStateFlow()
    private var native: SearchRecoveryCalls? = null
    private var closing = false
    private val released = CompletableDeferred<Result<Unit>>()

    fun prepare(expected: SearchRecoveryOpening?): Deferred<Unit>? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired || before.opening !== expected) return@synchronized null
        current.value = before.copy(opening = null, saved = null)
        run {
            val previous = native
            native = null
            previous?.close?.invoke()
            native = create()
            current.value.copy(
                opening = native?.let { SearchRecoveryOpening() },
                saved = null
            )
        }
    }

    fun confirm(expected: SearchRecoveryOpening): Deferred<Unit>? = synchronized(monitor) {
        val before = current.value
        if (!admitted(before, expected) || before.saved != null || before.failure != null) return@synchronized null
        val calls = native ?: return@synchronized null
        run { current.value.copy(saved = calls.confirm()) }
    }

    fun retry(expected: SearchRecoveryOpening): Deferred<Unit>? = synchronized(monitor) {
        val before = current.value
        if (!admitted(before, expected) || before.saved?.searchPending != true) return@synchronized null
        val calls = native ?: return@synchronized null
        run { current.value.copy(saved = calls.retry()) }
    }

    private fun admitted(before: SearchRecoveryView, expected: SearchRecoveryOpening) =
        !before.busy && !before.retired && before.opening === expected

    private fun run(action: () -> SearchRecoveryView): Deferred<Unit> {
        val done = CompletableDeferred<Unit>()
        current.value = current.value.copy(busy = true, failure = null)
        worker.launch {
            val outcome = runCatching(action)
            synchronized(monitor) {
                val retired = current.value.retired
                current.value = outcome.getOrElse { current.value.copy(failure = it) }
                    .copy(busy = false, retired = retired)
            }
            done.complete(Unit)
            releaseIfRetired()
        }
        return done
    }

    /** Retiring never cancels an admitted blocking native write or closes its handle underneath it. */
    fun retire(): Deferred<Result<Unit>> {
        synchronized(monitor) { current.value = current.value.copy(retired = true) }
        releaseIfRetired()
        return released
    }

    private fun releaseIfRetired() = synchronized(monitor) {
        if (current.value.retired && !current.value.busy && !closing) {
            closing = true
            worker.launch {
                val outcome = runCatching<Unit> { native?.close?.invoke() }
                released.complete(outcome)
                worker.cancel()
            }
        }
    }
}
