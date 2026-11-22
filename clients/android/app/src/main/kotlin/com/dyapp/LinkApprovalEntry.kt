package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

@Composable
internal fun LinkApprovalEntry(
    opened: Opened.Ready,
    enabled: Boolean = true,
    create: () -> LinkApprovalSession = { nativeLinkApprovalSession(opened) }
) = key(opened) {
    val owner = remember { LinkApprovalDialogOwner(create) }
    DisposableEffect(owner) { onDispose { owner.retire() } }
    val open = { if (enabled) owner.open() }
    TextButton(
        open,
        Modifier.fillMaxWidth().testTag("owner-open").applicationClick(open),
        enabled && owner.current == null
    ) { Text(stringResource(R.string.owner_link)) }
    owner.current?.let { session ->
        key(session) { LinkApprovalDialog(session, opened.peerId) { owner.dismiss(session) } }
    }
}
