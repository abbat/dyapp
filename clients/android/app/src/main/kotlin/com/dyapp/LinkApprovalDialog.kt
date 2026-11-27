package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties

@Composable
internal fun LinkApprovalDialog(session: LinkApprovalSession, identity: String, dismiss: () -> Unit) {
    val view by session.state.collectAsState()
    val draft = remember(session) { mutableStateOf("") }
    val title = when {
        view.delivery?.result?.isSuccess == true -> R.string.owner_sent_title
        view.opening != null -> R.string.owner_confirm_title
        else -> R.string.owner_link
    }
    AlertDialog(
        onDismissRequest = dismiss,
        modifier = Modifier.imePadding().testTag("owner-dialog"),
        properties = DialogProperties(decorFitsSystemWindows = false),
        title = { Text(stringResource(title)) },
        text = { LinkApprovalBody(session, view, identity, draft) },
        confirmButton = { LinkApprovalSubmit(session, view, draft.value) },
        dismissButton = { LinkApprovalDismiss(view, dismiss) }
    )
}

@Composable
private fun LinkApprovalBody(
    session: LinkApprovalSession,
    view: LinkApprovalViewState,
    identity: String,
    draft: MutableState<String>
) {
    val scroll = key(view.opening) { rememberScrollState() }
    Column(Modifier.testTag("owner-body").verticalScroll(scroll), Arrangement.spacedBy(12.dp)) {
        val opening = view.opening
        if (opening == null) {
            LinkApprovalInput(session, view, draft)
        } else {
            DeviceIdentifier(identity, "owner-identity", R.string.owner_identity)
            DeviceIdentifier(opening.code, "owner-recipient", R.string.owner_code_label)
            Text(stringResource(R.string.owner_confirm_text), Modifier.testTag("owner-consequences"))
            Text(stringResource(R.string.owner_confirm_warn), Modifier.testTag("owner-possession"))
        }
        LinkApprovalOutcome(view)
    }
}

@Composable
private fun LinkApprovalInput(
    session: LinkApprovalSession,
    view: LinkApprovalViewState,
    draft: MutableState<String>
) {
    Text(stringResource(R.string.owner_intro))
    OutlinedTextField(
        draft.value,
        { text -> if (!session.state.value.busy && !session.state.value.retired) draft.value = text },
        Modifier.fillMaxWidth().testTag("owner-code-input"),
        enabled = !view.busy && !view.retired,
        label = { Text(stringResource(R.string.owner_code_label)) },
        minLines = 4,
        maxLines = 4
    )
}
