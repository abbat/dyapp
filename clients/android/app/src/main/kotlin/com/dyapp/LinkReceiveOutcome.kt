package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkFailure
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt

/** Presentation of typed native phases, never diagnostic-string authorization. */
internal fun linkReceiveStatus(snapshot: LinkReceiverSnapshot, code: MobileLinkCodeStatus?): Int = when {
    snapshot.enrollment.completed -> R.string.link_setup_complete
    snapshot.busy && snapshot.receipt is MobileLinkReceipt.Installed -> R.string.link_setup_running
    code == MobileLinkCodeStatus.EXPIRED -> R.string.link_receive_expired
    snapshot.error != null -> R.string.link_receive_error
    else -> snapshot.phase?.let(::linkReceivePhaseStatus) ?: R.string.link_receive_starting
}

private fun linkReceivePhaseStatus(phase: MobileLinkPhase): Int = when (phase) {
    MobileLinkPhase.WAITING -> R.string.link_receive_waiting
    MobileLinkPhase.RECEIVING -> R.string.link_receive_receiving
    MobileLinkPhase.INSTALLING -> R.string.link_receive_installing
    MobileLinkPhase.COMPLETED -> R.string.link_receive_received
    MobileLinkPhase.CANCELLED -> R.string.link_receive_cancelled
    MobileLinkPhase.INVALIDATED -> R.string.link_receive_invalidated
    MobileLinkPhase.FAILED, MobileLinkPhase.INTERRUPTED -> R.string.link_receive_reconcile
}

@Composable
internal fun LinkReceiveOutcome(snapshot: LinkReceiverSnapshot) {
    when (val receipt = snapshot.receipt) {
        is MobileLinkReceipt.Installed -> {
            Text(receipt.selection.key.peerId(), Modifier.testTag("link-receive-peer"))
            if (!snapshot.enrollment.completed) Text(stringResource(R.string.link_receive_pending))
            Caption(stringResource(R.string.link_setup_no_publish))
            LinkEnrollmentNotice(snapshot.enrollment)
            if (receipt.selection.durabilityDetail != null) Text(stringResource(R.string.identity_persistence_notice))
        }
        is MobileLinkReceipt.Failed -> { LinkReceiveFailure(receipt.failure) }
        null -> {}
    }
    snapshot.error?.let {
        ErrorNotice(
            stringResource(R.string.link_receive_error),
            "link-receive-error",
            it.toString(),
            "link-receive-details",
            "link-receive-copy-details"
        )
    }
}

@Composable
private fun LinkReceiveFailure(failure: MobileLinkFailure) {
    val (label, detail) = when (failure) {
        is MobileLinkFailure.Admission -> R.string.link_receive_admission to failure.detail
        is MobileLinkFailure.Reservation -> R.string.link_receive_reservation to failure.detail
        is MobileLinkFailure.Unconfirmed -> R.string.link_receive_unconfirmed to failure.detail
        is MobileLinkFailure.Destination -> R.string.link_receive_destination to failure.detail
    }
    ErrorNotice(stringResource(label), "link-receive-failure", detail, "link-failure-details", "link-failure-copy")
    if (failure is MobileLinkFailure.Destination) Text(failure.slotId, Modifier.testTag("link-receive-destination"))
}
