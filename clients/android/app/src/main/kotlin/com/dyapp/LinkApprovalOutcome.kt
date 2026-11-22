package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import uniffi.dyapp.MobileLinkCodeStatus

@Composable
internal fun LinkApprovalOutcome(view: LinkApprovalViewState) {
    for ((tag, resource) in approvalMessages(view)) {
        Text(
            stringResource(resource),
            Modifier.testTag(tag).semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
}

private fun deliveryMessage(delivery: LinkApprovalDelivery): Pair<String, Int> = when (delivery.result.isSuccess) {
    true -> "owner-sent" to R.string.owner_sent
    false -> "owner-uncertain" to R.string.owner_uncertain
}

private fun pendingApprovalMessage(operation: LinkApprovalOperation?): Int? = when (operation) {
    LinkApprovalOperation.PREPARE -> R.string.owner_preparing
    LinkApprovalOperation.SEND -> R.string.owner_sending
    null -> null
}

private fun approvalMessages(view: LinkApprovalViewState): List<Pair<String, Int>> = buildList {
    val pending = pendingApprovalMessage(view.operation)
    if (pending != null) add("owner-pending" to pending)
    view.delivery?.let { add(deliveryMessage(it)) }
    when (view.codeStatus) {
        MobileLinkCodeStatus.MALFORMED -> add("owner-malformed" to R.string.owner_code_bad)
        MobileLinkCodeStatus.EXPIRED -> add("owner-expired" to R.string.owner_code_expired)
        else -> Unit
    }
    if (view.failure != null) {
        val resource = if (view.opening == null) R.string.owner_prepare_error else R.string.owner_unavailable
        add("owner-error" to resource)
    }
}
