package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * Six Settings categories preserve native panels and separate destructive collection actions.
 * Add changes the active slot through [act]; the lock row uses [lockMode] and [onLock].
 * Switching and the network remain in the shell; detail pages provide their own back route.
 */
@Composable
@Suppress("LongParameterList") // The section needs the slots, the lock mode and both routes out.
internal fun IdentitySettingsScreen(
    slots: Slots,
    active: Opened.Ready,
    lockMode: LockMode,
    act: (() -> Opened) -> Unit,
    onLock: () -> Unit
) {
    val fallback = rememberSettingsPane(active.slot.id)
    val route = LocalSettingsPane.current ?: fallback
    SettingsFrame(route) { pane ->
        when (pane) {
            SettingsPane.APPEARANCE -> AppearanceCard()
            SettingsPane.PRIVACY -> PrivacySettings(active, route)
            SettingsPane.NOTIFICATIONS -> NoticeCard(active)
            SettingsPane.PHOTOS -> PhotoSettings(active)
            SettingsPane.ACCOUNT -> AccountSettings(lockMode, route, onLock)
            SettingsPane.ABOUT -> AboutSettings()
            else -> SettingsCollection(pane, slots, active, act)
        }
    }
}

@Composable
private fun PhotoSettings(active: Opened.Ready) {
    PhotoDownloadCard(active)
    MediaCacheCard(active)
}

@Composable
private fun SettingsCollection(pane: SettingsPane, slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) {
    when (pane) {
        SettingsPane.IDENTITIES -> IdentityList(slots, active, act)
        SettingsPane.BLOCKED -> BlockedCard(active)
        SettingsPane.PASSED -> PassedCard(active)
        else -> Unit
    }
}

@Composable
private fun PrivacySettings(active: Opened.Ready, route: androidx.compose.runtime.MutableState<SettingsPane>) {
    ReadReceiptsRow(active)
    SettingsLink(SettingsPane.BLOCKED, route)
    SettingsLink(SettingsPane.PASSED, route)
}

@Composable
private fun AccountSettings(
    mode: LockMode,
    route: androidx.compose.runtime.MutableState<SettingsPane>,
    onLock: () -> Unit
) {
    SettingsLink(SettingsPane.IDENTITIES, route)
    LockRow(mode, onLock)
}

@Composable
private fun AboutSettings() {
    UpdatesCard()
    Caption(stringResource(R.string.settings_version, BuildConfig.VERSION_NAME))
}

@Composable
private fun IdentityList(slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) {
    // Closing the dialog reads the list again: removing another identity keeps the active slot.
    var removing by remember { mutableStateOf<Opened?>(null) }
    val loaded by rememberNativeRead(slots, active, removing) { RustBridge.show(slots) { Opened.Picker } }
    val listed = loaded?.getOrNull()
    FormCard(Modifier.testTag("settings-identities")) {
        CardTitle(R.string.settings_identities)
        when {
            loaded == null -> Caption(stringResource(R.string.view_loading))
            listed?.opened == null -> Text(stringResource(R.string.storage_error))
            else -> listed.entries.forEachIndexed { index, entry ->
                IdentitySettingsRow(entry, active.slot.id, index + 1) { removing = entry }
            }
        }
        Caption(stringResource(R.string.backup_none))
        Button(onClick = { act(slots::create) }, Modifier.testTag("settings-create")) {
            Text(stringResource(R.string.identity_add))
        }
    }
    removing?.let { target ->
        // Only removing the active identity leaves Settings for the picker.
        val stay = (target as? Opened.Ready)?.slot?.id != active.slot.id
        RemoveDialog(slots, target, { action ->
            removing = null
            act { action().let { ended -> if (stay) slots.open() else ended } }
        }) { removing = null }
    }
}

@Composable
private fun IdentitySettingsRow(entry: Opened, activeSlotId: String, number: Int, onRemove: () -> Unit) {
    val (slot, peerId) = when (entry) {
        is Opened.Ready -> entry.slot to entry.peerId
        is Opened.Recovery -> entry.slot to entry.peerId
        else -> return
    }
    if (slot == null) return
    val isActive = slot.id == activeSlotId
    Row(
        Modifier.fillMaxWidth().testTag("settings-identity"),
        Arrangement.spacedBy(12.dp),
        Alignment.CenterVertically
    ) {
        Avatar(peerId.orEmpty())
        Column(Modifier.weight(1f)) {
            peerId?.let { Text(shortPeerId(it), fontFamily = FontFamily.Monospace) }
            IdentityStatus(entry, isActive)
        }
        if (removable(entry)) {
            val tag = if (isActive) "settings-remove" else "settings-remove-$number"
            TextButton(onClick = onRemove, Modifier.testTag(tag)) {
                Text(stringResource(R.string.identity_remove))
            }
        }
    }
}

@Composable
private fun IdentityStatus(entry: Opened, isActive: Boolean) = when {
    entry is Opened.Recovery -> Caption(reasonText(entry.reason))
    isActive -> Caption(stringResource(R.string.settings_active_key), Modifier.testTag("settings-active"))
    else -> Caption(stringResource(R.string.settings_key_store))
}

@Composable
private fun LockRow(mode: LockMode, onClick: () -> Unit) = Surface(
    onClick = onClick,
    modifier = Modifier.fillMaxWidth().testTag("lock-settings"),
    shape = RoundedCornerShape(12.dp),
    color = MaterialTheme.colorScheme.surfaceContainer,
) {
    Row(Modifier.padding(16.dp), Arrangement.spacedBy(16.dp), Alignment.CenterVertically) {
        Icon(painterResource(R.drawable.ic_lock), contentDescription = null, Modifier.size(24.dp))
        Column {
            Text(stringResource(R.string.lock_settings), fontWeight = FontWeight.Bold)
            Caption(stringResource(lockModeText(mode)))
        }
    }
}
