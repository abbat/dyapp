package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileDeviceReadState
import uniffi.dyapp.MobileDeviceRowSource
import uniffi.dyapp.MobileDeviceSnapshot
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileOwnDevice
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference

/** Controlled core-shaped outcomes for rendering only; no node, native transfer or authorization proof. */
internal class OwnerVisualBackend(val state: OwnerVisualState) {
    val entered = CompletableDeferred<Unit>()
    private val release = CountDownLatch(1)
    val inspect = AtomicReference<() -> MobileLinkCodeStatus>({ initialStatus() })
    val approval = LinkApprovalSession({ inspect.get().invoke() }, ::prepare)
    val directory = DeviceDirectorySession {
        DeviceDirectoryCalls(::read, { error("Visual fixtures cannot revoke devices") }, {})
    }

    private fun initialStatus(): MobileLinkCodeStatus = when (state) {
        OwnerVisualState.MALFORMED -> MobileLinkCodeStatus.MALFORMED
        OwnerVisualState.EXPIRED -> MobileLinkCodeStatus.EXPIRED
        else -> MobileLinkCodeStatus.VALID
    }

    private fun prepare(input: String): LinkApprovalCalls {
        check(input.isNotBlank())
        if (state == OwnerVisualState.PREPARING) hold()
        check(state != OwnerVisualState.PREPARE_ERROR) { "Controlled read-only preparation failure" }
        return LinkApprovalCalls(OWNER_VISUAL_CODE, ::send, {})
    }

    private fun send() {
        if (state == OwnerVisualState.SENDING) hold()
        check(
            state !in setOf(
                OwnerVisualState.UNCERTAIN,
                OwnerVisualState.UNCERTAIN_EXPIRED,
                OwnerVisualState.UNCERTAIN_CHECK_ERROR
            )
        ) { "Controlled failure after an earlier encrypted part" }
    }

    private fun hold() {
        entered.complete(Unit)
        release.await()
    }

    private fun read(live: Boolean): MobileDeviceSnapshot = MobileDeviceSnapshot(
        listOf(
            MobileOwnDevice(ByteArray(32) { 1 }, true),
            MobileOwnDevice(ByteArray(32) { 2 }, false)
        ),
        if (live) MobileDeviceReadState.VERIFIED else MobileDeviceReadState.CACHED,
        if (live) MobileDeviceRowSource.REMOTE else MobileDeviceRowSource.SAVED
    )

    fun close() {
        release.countDown()
        runBlocking {
            try {
                withTimeout(5_000) { approval.retire().await().getOrThrow() }
            } finally {
                withTimeout(5_000) { directory.retire().await().getOrThrow() }
            }
        }
    }
}
