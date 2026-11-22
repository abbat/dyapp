package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * Six Settings categories preserve native panels and separate destructive collection actions.
 * Add changes the active slot through [actions]; the lock row uses [lockMode].
 * Switching and the network remain in the shell; detail pages provide their own back route.
 */
@Composable
internal fun IdentitySettingsScreen(
    slots: Slots,
    active: Opened.Ready,
    lockMode: LockMode,
    actions: IdentitySettingsActions
) {
    val fallback = rememberSettingsPane(active.slot.id)
    val route = LocalSettingsPane.current ?: fallback
    SettingsFrame(route) { pane ->
        when (pane) {
            SettingsPane.APPEARANCE -> AppearanceCard()
            SettingsPane.PRIVACY -> PrivacySettings(active, route)
            SettingsPane.NOTIFICATIONS -> NoticeCard(active)
            SettingsPane.PHOTOS -> PhotoSettings(active)
            SettingsPane.ACCOUNT -> AccountSettings(active, lockMode, route, actions.lock)
            SettingsPane.ABOUT -> AboutSettings()
            else -> SettingsCollection(route, slots, active, actions.act)
        }
    }
}

@Composable
private fun PhotoSettings(active: Opened.Ready) {
    PhotoDownloadCard(active)
    MediaCacheCard(active)
    SavedStickerManagerButton()
}

@Composable
private fun SettingsCollection(
    route: androidx.compose.runtime.MutableState<SettingsPane>,
    slots: Slots,
    active: Opened.Ready,
    act: (() -> Opened) -> Unit
) {
    when (route.value) {
        SettingsPane.IDENTITIES -> IdentityCollection(slots, active, act)
        SettingsPane.DEVICES -> DeviceDirectoryScreen(active)
        SettingsPane.RECEIVE -> LinkReceiveScreen(
            LinkReceiveSource(active),
            LinkReceiveNavigation(
                { route.value = SettingsPane.ACCOUNT },
                {
                    route.value = SettingsPane.ACCOUNT
                    act(slots::open)
                },
                { route.value = SettingsPane.IDENTITIES }
            )
        )
        SettingsPane.BLOCKED -> BlockedCard(active)
        SettingsPane.PASSED -> PassedCard(active)
        else -> Unit
    }
}

@Composable
private fun PrivacySettings(active: Opened.Ready, route: androidx.compose.runtime.MutableState<SettingsPane>) {
    PrivacyPreferenceRow(active, PrivacyPreferenceKind.VIEWS)
    ReadReceiptsRow(active)
    PrivacyPreferenceRow(active, PrivacyPreferenceKind.TYPING)
    PresenceCard(active)
    SettingsLink(SettingsPane.BLOCKED, route)
    SettingsLink(SettingsPane.PASSED, route)
}

@Composable
private fun AccountSettings(
    active: Opened.Ready,
    mode: LockMode,
    route: androidx.compose.runtime.MutableState<SettingsPane>,
    onLock: () -> Unit
) {
    SettingsLink(SettingsPane.IDENTITIES, route)
    DeviceDirectoryMenu(active, route)
    SettingsLink(SettingsPane.RECEIVE, route)
    LockRow(mode, onLock)
}

@Composable
private fun AboutSettings() {
    UpdatesCard()
    Caption(stringResource(R.string.settings_version, BuildConfig.VERSION_NAME))
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
