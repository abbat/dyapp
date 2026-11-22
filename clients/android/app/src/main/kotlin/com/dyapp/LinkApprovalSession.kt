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
import uniffi.dyapp.MobileLinkCodeStatus

/** Presentation lifetime/admission only; core owns validation, expiry, consent admission and transfer. */
internal class LinkApprovalSession(
    private val inspect: (String) -> MobileLinkCodeStatus,
    private val create: (String) -> LinkApprovalCalls
) {
    private val monitor = Any()
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val current = MutableStateFlow(LinkApprovalViewState())
    val state = current.asStateFlow()
    private var native: LinkApprovalCalls? = null
    private var closing = false
    private val released = CompletableDeferred<Result<Unit>>()

    fun prepare(text: String): Deferred<Unit>? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired || before.opening != null) return@synchronized null
        val done = CompletableDeferred<Unit>()
        current.value = before.copy(operation = LinkApprovalOperation.PREPARE, codeStatus = null, failure = null)
        worker.launch {
            var status: MobileLinkCodeStatus? = null
            val outcome = runCatching {
                status = inspect(text)
                if (status == MobileLinkCodeStatus.VALID) {
                    val prepared = create(text)
                    native = prepared
                    LinkApprovalOpening(prepared.code)
                } else {
                    null
                }
            }
            synchronized(monitor) {
                current.value = current.value.copy(
                    opening = outcome.getOrNull(), codeStatus = status,
                    operation = null, failure = outcome.exceptionOrNull()
                )
            }
            done.complete(Unit)
            releaseIfRetired()
        }
        done
    }

    fun confirm(expected: LinkApprovalOpening): Deferred<Unit>? = synchronized(monitor) {
        val before = current.value
        if (before.busy || before.retired || before.opening !== expected) return@synchronized null
        if (before.codeStatus != MobileLinkCodeStatus.VALID || before.delivery?.result?.isSuccess == true) {
            return@synchronized null
        }
        val prepared = native ?: return@synchronized null
        val done = CompletableDeferred<Unit>()
        current.value = before.copy(operation = LinkApprovalOperation.SEND, failure = null)
        worker.launch {
            val checked = runCatching { inspect(expected.code) }
            val delivery = if (checked.getOrNull() == MobileLinkCodeStatus.VALID) {
                LinkApprovalDelivery(expected, runCatching { prepared.send() })
            } else {
                null
            }
            synchronized(monitor) {
                // A later expiry/read failure never erases an earlier potentially partial delivery.
                current.value = current.value.copy(
                    codeStatus = checked.getOrNull(), operation = null,
                    delivery = delivery ?: current.value.delivery, failure = checked.exceptionOrNull()
                )
            }
            done.complete(Unit)
            releaseIfRetired()
        }
        done
    }

    /** Retire UI callbacks now; release the handle only after an admitted blocking call completes. */
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
                synchronized(monitor) {
                    current.value = current.value.copy(failure = current.value.failure ?: outcome.exceptionOrNull())
                }
                released.complete(outcome)
                worker.cancel()
            }
        }
    }
}
