package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.dyapp.LocalCollectionEntry
import uniffi.dyapp.MobileDeviceReadState
import uniffi.dyapp.MobileDeviceRowSource

@Composable
internal fun DeviceDirectoryScreen(opened: Opened.Ready) = key(opened) {
    val session = remember { DeviceDirectorySession { nativeDeviceDirectoryCalls(opened) } }
    DeviceDirectoryScreen(session) { enabled -> LinkApprovalEntry(opened, enabled) }
}

/** A fresh screen owns fresh presentation; retiring it cannot cancel admitted native work. */
@Composable
internal fun DeviceDirectoryScreen(
    session: DeviceDirectorySession,
    ownerActions: @Composable (Boolean) -> Unit = {}
) = key(session) {
    val view by session.state.collectAsState()
    DisposableEffect(session) { onDispose { session.retire() } }
    LaunchedEffect(session) {
        if (session.refresh(false)?.await()?.isSuccess == true) session.refresh(true)?.await()
    }
    DeviceDirectoryContent(session, view)
    ownerActions(!view.busy && !view.retired && view.confirmation == null)
    view.confirmation?.let { opening ->
        key(opening) { DeviceDisconnectDialog(session, opening, view) }
    }
}

@Composable
private fun DeviceDirectoryContent(session: DeviceDirectorySession, view: DeviceDirectoryViewState) {
    val listing = view.listing
    if (listing != null) {
        Caption(stringResource(deviceReadText(listing)), Modifier.testTag("device-read-status"))
    }
    if (view.failure != null) {
        Text(stringResource(R.string.devices_read_error), Modifier.testTag("devices-read-error"))
    }
    DeviceDirectoryCompletion(view.completion)
    FormCard(Modifier.testTag("devices")) {
        DeviceDirectoryRows(session, view)
        Caption(stringResource(R.string.devices_hint))
    }
}

private fun deviceReadText(listing: DeviceDirectoryListing): Int = when (listing.state) {
    MobileDeviceReadState.CACHED -> R.string.devices_read_saved
    MobileDeviceReadState.UNAVAILABLE -> R.string.devices_read_unavailable
    MobileDeviceReadState.NO_PROFILE -> R.string.devices_read_no_profile
    MobileDeviceReadState.VERIFIED -> when (listing.source) {
        MobileDeviceRowSource.SAVED -> {
            R.string.devices_read_verified_saved
        }
        MobileDeviceRowSource.REMOTE -> {
            if (listing.rows.isEmpty()) R.string.devices_read_empty else R.string.devices_read_verified
        }
    }
}

@Composable
private fun DeviceDirectoryRows(session: DeviceDirectorySession, view: DeviceDirectoryViewState) {
    val records = remember(view.listing) { view.listing?.rows?.associateBy { it.identifier } }
    val entries = remember(records) { records?.map { (id, _) -> LocalCollectionEntry(id, id) } }
    val refresh: () -> Unit = { session.refresh(true) }
    if (view.listing == null && view.failure != null) {
        TextButton(refresh, Modifier.testTag("collection-refresh"), !view.busy) {
            Text(stringResource(R.string.collection_refresh))
        }
    } else {
        LocalCollectionView(session, entries, if (view.busy) null else refresh) { row, number ->
            records?.get(row.target)?.let { device ->
                DeviceDirectoryRowView(device, number, view.busy, session)
            }
        }
    }
}

@Composable
private fun DeviceDirectoryRowView(
    row: DeviceDirectoryRow,
    number: Int,
    busy: Boolean,
    session: DeviceDirectorySession
) = Column(
    Modifier.fillMaxWidth().testTag("device-row-" + row.identifier),
    Arrangement.spacedBy(12.dp)
) {
    val request: () -> Unit = { session.request(row) }
    DeviceIdentifier(row.identifier, "device-key-$number")
    Caption(stringResource(if (row.current) R.string.devices_this else R.string.devices_other))
    if (!row.current) {
        TextButton(request, Modifier.testTag("device-disconnect-$number").applicationClick(request), !busy) {
            Text(stringResource(R.string.devices_disconnect))
        }
    }
}

@Composable
private fun DeviceDirectoryCompletion(completion: DeviceDisconnectCompletion?) {
    if (completion == null) return
    val text = if (completion.result.isSuccess) R.string.devices_gone else R.string.devices_uncertain
    Text(
        stringResource(text),
        Modifier.testTag("devices-completion").semantics { liveRegion = LiveRegionMode.Polite }
    )
    DeviceIdentifier(completion.target.identifier, "devices-completion-target")
}
