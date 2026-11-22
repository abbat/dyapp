package com.dyapp

import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import uniffi.dyapp.MobileLinkCodeStatus

@Composable
internal fun LinkApprovalOutcome(view: LinkApprovalViewState) {
    val messages = approvalMessages(view)
    for ((index, message) in messages.withIndex()) {
        key(message.first) { LinkApprovalMessage(message, view, index == 0) }
    }
}

@Composable
private fun LinkApprovalMessage(
    message: Pair<String, Int>,
    view: LinkApprovalViewState,
    first: Boolean
) {
    val reveal = remember { BringIntoViewRequester() }
    var placed by remember { mutableStateOf(false) }
    var line by remember { mutableStateOf<Rect?>(null) }
    val ready = !view.retired && placed && LocalWindowInfo.current.isWindowFocused
    Text(
        stringResource(message.second),
        Modifier.testTag(message.first).bringIntoViewRequester(reveal)
            .onGloballyPositioned { placed = it.isAttached }
            .semantics { liveRegion = LiveRegionMode.Polite },
        onTextLayout = { layout ->
            line = Rect(0f, layout.getLineTop(0), layout.size.width.toFloat(), layout.getLineBottom(0))
        }
    )
    LaunchedEffect(view, ready, line) {
        if (first && ready) line?.let { reveal.bringIntoView(it) }
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
