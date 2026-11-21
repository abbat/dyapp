package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily

internal class NetworkWizardNavigation(val done: () -> Unit, val cancel: (() -> Unit)? = null)

/** Receiving cannot skip activation; Cancel leaves the explicitly created identity and draft intact. */
@Composable
internal fun NetworkReceiveWizard(opened: Opened.Ready, cancel: () -> Unit, done: () -> Unit) =
    NetworkDraftContent(opened, NetworkWizardNavigation(done, cancel))

@Composable
internal fun WizardNetworkHeading(opened: Opened.Ready, navigation: NetworkWizardNavigation) {
    if (navigation.cancel != null) {
        PageTitle(R.string.link_receive_network_title)
        Text(stringResource(R.string.link_receive_network_intro))
    } else {
        Caption(stringResource(R.string.wizard_step2))
        PageTitle(R.string.wizard_network_title)
        Caption(stringResource(R.string.storage_peer_id))
        Text(opened.peerId, Modifier.testTag("peer-id"), fontFamily = FontFamily.Monospace)
        CopyButton(opened.peerId, "copy-peer-id")
    }
}

@Composable
internal fun WizardNetworkExit(state: NetworkDraftState, navigation: NetworkWizardNavigation) {
    val receiving = navigation.cancel != null
    TextButton(
        onClick = navigation.cancel ?: navigation.done,
        Modifier.fillMaxWidth().testTag(if (receiving) "cancel-receive-network" else "skip-network"),
        state.checkStatus != R.string.network_activating
    ) {
        Text(stringResource(if (receiving) R.string.search_cancel else R.string.wizard_skip))
    }
}
