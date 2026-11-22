package com.dyapp

import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkFailure
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.MobileSlotSelection

/** Controlled presentation outcomes; only keys and invitation QR come from the actual core. */
internal class ReceiveVisualFixture(
    val sourcePeer: String,
    private val selection: MobileSlotSelection,
    private val invitation: LinkReceiverDisplay
) {
    val actions = LinkReceiveActions(
        {},
        {},
        {},
        {},
        {},
        LinkEnrollmentActions({}, {}, { _, _ -> null })
    )

    fun status(state: ReceiveVisualState) =
        if (state == ReceiveVisualState.EXPIRED) MobileLinkCodeStatus.EXPIRED else MobileLinkCodeStatus.VALID

    fun snapshot(state: ReceiveVisualState): LinkReceiverSnapshot {
        val beforeInvitation = state.phase == null
        return LinkReceiverSnapshot(
            display = if (beforeInvitation) null else invitation,
            phase = state.phase,
            receipt = receipt(state),
            error = if (state in listOf(ReceiveVisualState.START_ERROR, ReceiveVisualState.POLL_ERROR)) {
                IllegalStateException("controlled transport failure; original attempt retained")
            } else {
                null
            },
            busy = state in listOf(
                ReceiveVisualState.STARTING, ReceiveVisualState.RECEIVING,
                ReceiveVisualState.INSTALLING, ReceiveVisualState.SETUP_RUNNING
            ),
            retired = state in listOf(ReceiveVisualState.CANCELLED, ReceiveVisualState.EXPIRED),
            enrollment = LinkEnrollmentSnapshot(
                completed = state == ReceiveVisualState.COMPLETE,
                failure = enrollmentFailure(state),
                operation = if (state == ReceiveVisualState.SETUP_RUNNING) LinkEnrollmentOperation.FINISH else null
            )
        )
    }

    private fun receipt(state: ReceiveVisualState): MobileLinkReceipt? {
        if (state.installed) {
            val selected = if (state == ReceiveVisualState.DURABILITY) {
                selection.copy(durabilityDetail = "controlled directory persistence failure")
            } else {
                selection
            }
            return MobileLinkReceipt.Installed(selected)
        }
        return failure(state)?.let { MobileLinkReceipt.Failed(it) }
    }

    private fun failure(state: ReceiveVisualState): MobileLinkFailure? = when (state) {
        ReceiveVisualState.ADMISSION -> MobileLinkFailure.Admission("controlled admission refusal")
        ReceiveVisualState.RESERVATION -> MobileLinkFailure.Reservation("controlled reservation refusal")
        ReceiveVisualState.DESTINATION -> MobileLinkFailure.Destination(
            "reserved-" + "d".repeat(128),
            "controlled destination refusal"
        )
        ReceiveVisualState.UNCONFIRMED -> MobileLinkFailure.Unconfirmed("controlled unconfirmed writer")
        else -> null
    }

    private fun enrollmentFailure(state: ReceiveVisualState): Throwable? = when (state) {
        ReceiveVisualState.REVIEW -> LinkReviewException.ReviewRequired()
        ReceiveVisualState.PRIVATE_REVIEW -> LinkReviewException.PrivateStateReviewRequired()
        ReceiveVisualState.AUTHORIZATION -> LinkReviewException.EnrollmentAuthorizationRequired()
        ReceiveVisualState.REVOKED -> LinkReviewException.Revoked()
        ReceiveVisualState.UNLISTED -> LinkReviewException.DeviceNotListed()
        ReceiveVisualState.SETUP_ERROR -> LinkReviewException.Failed("controlled backend failure")
        else -> null
    }
}

private fun withReceiveVisualSelection(
    registry: MobileIdentityRegistry,
    block: (ReceiveVisualFixture) -> Unit
) {
    val selected = registry.create()
    val source = selected.key.use { Opened.Ready(selected.slot.view(registry), it.peerId()) }
    val receiver = linkReceiverCalls(source, false)
    try {
        val installed = registry.create()
        installed.key.use { block(ReceiveVisualFixture(source.peerId, installed, receiver.display)) }
    } finally {
        receiver.close()
    }
}

/** Never polls or connects: the invitation and its matrix are prepared entirely locally by Rust. */
internal fun withReceiveVisualFixture(block: (ReceiveVisualFixture) -> Unit) {
    NativeSlotFixture().use { files ->
        MobileIdentityRegistry(files.root.absolutePath, MobileSlotLayout.ANDROID, NativeSlotKeys).use { registry ->
            withReceiveVisualSelection(registry, block)
        }
    }
}
