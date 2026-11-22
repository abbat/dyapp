package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import java.text.DateFormat
import java.util.Date
import java.util.concurrent.TimeUnit

/** Receiving is explicit; an old operation remains visible until its exact outcome is disposed. */
@Composable
internal fun LinkReceiveScreen(
    source: LinkReceiveSource,
    navigation: LinkReceiveNavigation,
    store: LinkReceiverStore = pendingLinkReceiver,
    classify: suspend (String) -> MobileLinkCodeStatus? = ::readLinkCodeStatus
) {
    val opening by store.state.collectAsState()
    val actions = rememberLinkReceiveActions(source, opening, navigation, store)
    val active = opening
    if (active == null) {
        LinkReceiveIntro(actions, source.wizard)
        return
    }
    key(active) {
        val snapshot by active.session.state.collectAsState()
        val status by rememberLinkCodeStatus(active, classify)
        LinkReceiveContent(active.source.peerId, snapshot, status, actions)
    }
}

/** Render the observed outcome; admission and lifecycle ownership remain in the screen and core. */
@Composable
internal fun LinkReceiveContent(
    sourcePeer: String,
    snapshot: LinkReceiverSnapshot,
    status: MobileLinkCodeStatus?,
    actions: LinkReceiveActions
) {
    DeviceIdentifier(sourcePeer, "link-receive-source", R.string.link_receive_source)
    Text(
        stringResource(linkReceiveStatus(snapshot, status)),
        Modifier.testTag("link-receive-status").semantics { liveRegion = LiveRegionMode.Polite }
    )
    if (snapshot.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    LinkReceiveInvitation(snapshot, status)
    LinkReceiveOutcome(snapshot)
    if (snapshot.receipt is MobileLinkReceipt.Installed && !snapshot.enrollment.showsComparison) {
        LinkEnrollmentAction(snapshot, actions.enrollment)
    }
    LinkReviewPanel(snapshot, actions.enrollment)
    LinkReceiveButtons(snapshot, actions)
}

@Composable
internal fun LinkReceiveIntro(actions: LinkReceiveActions, wizard: Boolean) {
    Text(stringResource(if (wizard) R.string.link_receive_wizard_intro else R.string.link_receive_intro))
    Button(actions.start, Modifier.fillMaxWidth().testTag("link-receive-start").applicationClick(actions.start)) {
        Text(stringResource(R.string.link_receive_start))
    }
}

@Composable
private fun LinkReceiveInvitation(snapshot: LinkReceiverSnapshot, status: MobileLinkCodeStatus?) {
    val display = snapshot.display ?: return
    val waiting = snapshot.phase in setOf(MobileLinkPhase.WAITING, MobileLinkPhase.RECEIVING)
    if (status != MobileLinkCodeStatus.VALID || !waiting) return
    val locale = LocalConfiguration.current.locales[0]
    val expiry = remember(display.expiresAt, locale) {
        DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, locale)
            .format(Date(TimeUnit.SECONDS.toMillis(display.expiresAt.toLong())))
    }
    LinkQr(display.matrix)
    DeviceIdentifier(display.code, "link-receive-code", R.string.link_receive_code_label, "link-receive-copy")
    Caption(stringResource(R.string.link_receive_expiry, expiry))
}

@Composable
private fun LinkReceiveButtons(snapshot: LinkReceiverSnapshot, actions: LinkReceiveActions) {
    if (snapshot.phase in setOf(null, MobileLinkPhase.WAITING, MobileLinkPhase.RECEIVING) && !snapshot.retired) {
        TextButton(actions.cancel, Modifier.testTag("link-receive-cancel").applicationClick(actions.cancel)) {
            Text(stringResource(R.string.link_receive_cancel))
        }
    }
    if (snapshot.receipt is MobileLinkReceipt.Installed) {
        Button(
            actions.openApp,
            Modifier.testTag("link-receive-open").applicationClick(actions.openApp),
            !snapshot.busy
        ) {
            Text(stringResource(R.string.link_receive_open))
        }
    }
    LinkReceiveDisposition(snapshot, actions)
}

@Composable
private fun LinkReceiveDisposition(snapshot: LinkReceiverSnapshot, actions: LinkReceiveActions) {
    TextButton(
        actions.review,
        Modifier.testTag("link-receive-review").applicationClick(actions.review),
        !snapshot.busy
    ) {
        Text(stringResource(R.string.link_receive_review))
    }
    Caption(stringResource(R.string.link_receive_disposition))
    TextButton(
        actions.dismiss,
        Modifier.testTag("link-receive-dismiss").applicationClick(actions.dismiss),
        !snapshot.busy
    ) {
        Text(stringResource(R.string.link_receive_dismiss))
    }
}
