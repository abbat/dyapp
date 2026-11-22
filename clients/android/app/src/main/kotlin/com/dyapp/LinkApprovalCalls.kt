package com.dyapp

import uniffi.dyapp.MobileLinkApproval
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.deviceLinkCodeStatus

/** Public normalized target and blocking operations from the same prepared native handle. */
internal class LinkApprovalCalls(val code: String, val send: () -> Unit, val close: () -> Unit)

internal fun nativeLinkApprovalSession(opened: Opened.Ready): LinkApprovalSession =
    LinkApprovalSession(::deviceLinkCodeStatus) { nativeLinkApprovalCalls(opened, it) }

/** Getter failure must release a prepared handle too; no credentials are exported during preparation. */
internal fun nativeLinkApprovalCalls(opened: Opened.Ready, text: String): LinkApprovalCalls {
    val approval = MobileLinkApproval.prepare(opened.slot.owner(), opened.slot.id, opened.peerId, text)
    return runCatching {
        LinkApprovalCalls(approval.code(), approval::confirmAndSend, approval::close)
    }.onFailure { approval.close() }.getOrThrow()
}

/** Reference identity owns one consent; equal public text does not make another opening admissible. */
internal class LinkApprovalOpening(val code: String)

internal class LinkApprovalDelivery(val opening: LinkApprovalOpening, val result: Result<Unit>)

internal enum class LinkApprovalOperation {
    PREPARE,
    SEND
}

internal data class LinkApprovalViewState(
    val opening: LinkApprovalOpening? = null,
    val codeStatus: MobileLinkCodeStatus? = null,
    val operation: LinkApprovalOperation? = null,
    val delivery: LinkApprovalDelivery? = null,
    val failure: Throwable? = null,
    val retired: Boolean = false
) {
    val busy: Boolean get() = operation != null
}
