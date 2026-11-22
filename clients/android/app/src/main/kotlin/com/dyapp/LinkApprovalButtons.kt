package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.MobileLinkCodeStatus

@Composable
internal fun LinkApprovalSubmit(session: LinkApprovalSession, view: LinkApprovalViewState, input: String) {
    if (hideApprovalSubmit(view)) return
    val opening = view.opening
    val retry = view.delivery?.result?.isFailure == true
    val submit: () -> Unit = {
        if (opening == null) session.prepare(input) else session.confirm(opening)
    }
    val enabled = !view.busy && !view.retired &&
        (opening == null || view.codeStatus == MobileLinkCodeStatus.VALID)
    val label = approvalSubmitLabel(view)
    TextButton(
        submit,
        Modifier.testTag(if (retry) "owner-retry" else if (opening == null) "owner-code-next" else "owner-send")
            .applicationClick(submit),
        enabled
    ) { Text(stringResource(label)) }
}

private fun hideApprovalSubmit(view: LinkApprovalViewState): Boolean =
    view.delivery?.result?.isSuccess == true ||
        view.delivery?.result?.isFailure == true && view.codeStatus == MobileLinkCodeStatus.EXPIRED

private fun approvalSubmitLabel(view: LinkApprovalViewState): Int = when {
    view.opening == null -> R.string.owner_continue
    view.delivery?.result?.isFailure == true -> R.string.devices_retry
    else -> R.string.owner_confirm_action
}

@Composable
internal fun LinkApprovalDismiss(view: LinkApprovalViewState, dismiss: () -> Unit) {
    val focus = remember { FocusRequester() }
    val ready = LocalWindowInfo.current.isWindowFocused && view.operation != LinkApprovalOperation.SEND
    val sent = view.delivery?.result?.isSuccess == true
    TextButton(
        dismiss,
        Modifier.testTag(if (sent) "owner-done" else "owner-cancel")
            .focusRequester(focus)
            .focusProperties { canFocus = true }
            .applicationClick(dismiss),
        view.operation != LinkApprovalOperation.SEND && !view.retired
    ) { Text(stringResource(if (sent) R.string.owner_done else R.string.remove_cancel)) }
    LaunchedEffect(ready) { if (ready) focus.requestFocus() }
}
