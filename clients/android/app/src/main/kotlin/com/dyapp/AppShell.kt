package com.dyapp

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AssistChip
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.NetworkFile

/** A bottom bar section: its target, test tag, label and icon. */
private class Section(val target: StartTarget, val tag: String, val label: Int, val icon: Int)

private val sections = listOf(
    Section(StartTarget.PROFILE, "nav-profile", R.string.nav_profile, R.drawable.ic_nav_profile),
    Section(StartTarget.SEARCH, "nav-search", R.string.nav_search, R.drawable.ic_nav_search),
    Section(StartTarget.CHATS, "nav-chats", R.string.nav_chats, R.drawable.ic_nav_chats),
    Section(StartTarget.NETWORK, "nav-network", R.string.nav_network, R.drawable.ic_nav_network),
    Section(StartTarget.SETTINGS, "nav-settings", R.string.nav_settings, R.drawable.ic_nav_settings),
)

/**
 * The first-run network wizard while it applies, otherwise the ready identity's frame: a top bar
 * with the section title (`app-ready`), the active network and the identity avatar, the [section]
 * content and the bottom navigation bar. [open] switches
 * sections and opens the lock screen; [act] changes the active identity.
 */
@Composable
@Suppress("LongParameterList") // Sections and identity actions both route through the shell.
internal fun AppShell(
    slots: Slots,
    opened: Opened.Ready,
    section: StartTarget?,
    lockMode: LockMode,
    act: (() -> Opened) -> Unit,
    open: (StartTarget?) -> Unit,
) {
    if (showNetworkWizard(opened, section)) return
    // Above the chat branch: the mailbox poll goes on while a chat is open.
    val feed = rememberChatFeed(opened)
    val search = rememberSearchSession(opened)
    var chatPeer by rememberSaveable(opened.slot.id) { mutableStateOf<String?>(null) }
    var viewPeer by rememberSaveable(opened.slot.id) { mutableStateOf<String?>(null) }
    // The result already verified; after a recreation only the Peer ID is left and the page fetches.
    var viewFound by remember(opened.slot.id) { mutableStateOf<FoundProfile?>(null) }
    val closeView = { viewPeer = null; viewFound = null }
    val openFound = { found: FoundProfile -> viewFound = found; viewPeer = found.peerId }
    val chat = chatPeer
    val viewed = viewPeer
    if (viewed != null) {
        val backLabel = if (chat == null && section == StartTarget.SEARCH) R.string.view_back else R.string.lock_back
        val actions = ProfileActions(backLabel, closeView) {
            closeView()
            chatPeer = viewed
            // Back from a chat always lands on Chats, as the prototype's back button says.
            open(StartTarget.CHATS)
        }
        ProfileViewScreen(opened, viewed, viewFound, feed, actions)
    } else if (chat != null) {
        ChatScreen(opened, chat, feed, { viewPeer = chat }) { chatPeer = null }
    } else {
        ShellFrame(slots, opened, section, lockMode, act, open, feed, { chatPeer = it }, openFound) {
            SearchScreen(search, { open(StartTarget.NETWORK) }, openFound)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
@Suppress("LongParameterList") // The frame routes the shell's sections and identity actions.
private fun ShellFrame(
    slots: Slots,
    opened: Opened.Ready,
    section: StartTarget?,
    lockMode: LockMode,
    act: (() -> Opened) -> Unit,
    open: (StartTarget?) -> Unit,
    feed: ChatFeed,
    openChat: (String) -> Unit,
    openFound: (FoundProfile) -> Unit,
    search: @Composable () -> Unit,
) {
    var switcher by remember { mutableStateOf(false) }
    val current = sections.firstOrNull { it.target == section } ?: sections.first()
    Scaffold(
        topBar = { ShellTopBar(opened, current, open) { switcher = true } },
        bottomBar = { ShellNavigationBar(current, chatsBadge(feed.rows), open) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            if (current.target == StartTarget.CHATS) {
                ChatsScreen(feed, openChat, openFound)
            } else {
                SectionContent(slots, opened, current.target, lockMode, act, open, search)
            }
        }
    }
    if (switcher) {
        ModalBottomSheet(onDismissRequest = { switcher = false }, Modifier.testTag("identity-menu")) {
            IdentityMenu(slots, opened) { action -> switcher = false; act(action) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ShellTopBar(opened: Opened.Ready, current: Section, open: (StartTarget?) -> Unit, onSwitch: () -> Unit) {
    TopAppBar(
        title = { Text(stringResource(current.label), Modifier.testTag(APP_READY_TAG)) },
        actions = {
            ConnectionStatus(opened, current.target) { open(StartTarget.NETWORK) }
            val identities = stringResource(R.string.settings_identities)
            IconButton(onClick = onSwitch,
                Modifier.testTag("identity-switcher").semantics { contentDescription = identities }) {
                Avatar(opened.peerId)
            }
        },
    )
}

@Composable
private fun ShellNavigationBar(current: Section, chatsBadge: ULong, open: (StartTarget?) -> Unit) {
    NavigationBar {
        for (item in sections) {
            NavigationBarItem(
                selected = item === current,
                onClick = { open(item.target) },
                icon = {
                    BadgedBox(badge = {
                        if (item.target == StartTarget.CHATS && chatsBadge > 0uL) {
                            UnreadBadge(chatsBadge, Modifier.testTag("chats-unread"))
                        }
                    }) {
                        Icon(painterResource(item.icon), contentDescription = null)
                    }
                },
                label = { Text(stringResource(item.label)) },
                modifier = Modifier.testTag(item.tag),
            )
        }
    }
}

@Composable
@Suppress("LongParameterList") // Settings needs both identity actions and section routing.
private fun SectionContent(
    slots: Slots,
    opened: Opened.Ready,
    target: StartTarget,
    lockMode: LockMode,
    act: (() -> Opened) -> Unit,
    open: (StartTarget?) -> Unit,
    search: @Composable () -> Unit,
) {
    when (target) {
        StartTarget.SEARCH -> search()
        StartTarget.NETWORK -> NetworkDraftScreen(opened, null)
        StartTarget.SETTINGS -> IdentitySettingsScreen(slots, opened, lockMode, act) { open(StartTarget.LOCK) }
        else -> Column {
            Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp)) {
                Text(stringResource(R.string.storage_peer_id), style = MaterialTheme.typography.labelMedium)
                Text(opened.peerId, Modifier.testTag("peer-id"))
                // The OS gives the app no chance to warn before these erase the key.
                Text(stringResource(R.string.storage_os_loss), Modifier.testTag("os-loss"),
                    style = MaterialTheme.typography.bodySmall)
            }
            ProfileDraftScreen(opened)
        }
    }
}

// The active network by name, read again on each section change so an activation shows; opens Network.
@Composable
private fun ConnectionStatus(opened: Opened.Ready, section: StartTarget, onClick: () -> Unit) {
    val name by produceState<String?>(null, opened.slot.id, section) {
        value = withContext(Dispatchers.IO) {
            runCatching { RustBridge.loadNetwork(opened, NetworkFile.ACTIVE)?.name }.getOrNull()
        }
    }
    val public = stringResource(R.string.settings_network_public)
    AssistChip(onClick = onClick, modifier = Modifier.testTag("connection-status"),
        label = { Text(name?.ifEmpty { public } ?: stringResource(R.string.status_no_network)) })
}

private const val SHORT_PEER_ID = 6

internal fun shortPeerId(peerId: String) = "@" + peerId.takeLast(SHORT_PEER_ID)

// No photo yet: the first two Peer ID characters on a tonal circle.
@Composable
internal fun Avatar(peerId: String) {
    Box(
        Modifier.size(32.dp).clip(CircleShape).background(MaterialTheme.colorScheme.secondaryContainer),
        contentAlignment = Alignment.Center,
    ) {
        Text(peerId.take(2), style = MaterialTheme.typography.labelMedium)
    }
}

// The identities on this device: the active one, the others to switch to, and Add identity.
@Composable
private fun IdentityMenu(slots: Slots, active: Opened.Ready, act: (() -> Opened) -> Unit) {
    val listed = remember(slots, active.slot.id) { RustBridge.show(slots) { Opened.Picker } }
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.settings_identities), style = MaterialTheme.typography.titleMedium)
        listed.entries.forEachIndexed { index, entry ->
            val (slot, label) = when (entry) {
                is Opened.Ready -> entry.slot to entry.peerId
                is Opened.Recovery -> entry.slot to (entry.peerId ?: reasonText(entry.reason))
                else -> return@forEachIndexed
            }
            if (slot.id == active.slot.id) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(label, Modifier.weight(1f))
                    Text(stringResource(R.string.settings_active), style = MaterialTheme.typography.labelMedium)
                }
            } else {
                TextButton(onClick = { act { slots.setActive(slot.id); slots.open() } },
                    Modifier.testTag("switch-to-${index + 1}")) { Text(label) }
            }
        }
        TextButton(onClick = { act(slots::create) }, Modifier.testTag("add-identity-menu")) {
            Text(stringResource(R.string.identity_add))
        }
    }
}
