package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
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
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
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

private class ShellContext(
    val slots: Slots,
    val opened: Opened.Ready,
    val section: StartTarget?,
    val controls: ShellControls
)

private class ShellChatActions(val open: (String) -> Unit, val found: (FoundProfile) -> Unit)

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
 * content and the bottom navigation bar. [controls] routes sections, lock and identity changes.
 */
@Composable
internal fun AppShell(
    slots: Slots,
    opened: Opened.Ready,
    section: StartTarget?,
    controls: ShellControls,
) {
    val context = ShellContext(slots, opened, section, controls)
    if (showNetworkWizard(opened, section)) return
    // Above the chat branch: the mailbox poll goes on while a chat is open.
    val feed = rememberChatFeed(opened)
    ChatPresenceForeground(opened)
    val search = rememberSearchSession(opened)
    val settings = rememberSettingsPane(opened.slot.id)
    CompositionLocalProvider(
        LocalPeerPhotos provides search.photos,
        LocalSettingsPane provides settings,
        LocalActiveChatPeers provides feed.presence.peers
    ) {
        SavedStickerManagerHost(opened, remember(opened) { savedStickerCalls(opened) }) {
            ShellRoutes(context, feed, search)
        }
    }
}

// Profile pages over chats over the sections, each with the peer photos of the identity's searcher.
@Composable
private fun ShellRoutes(context: ShellContext, feed: ChatFeed, search: SearchSession) {
    val opened = context.opened
    val open = context.controls.open
    val peers = rememberShellPeers(opened.slot.id)
    val chat = peers.chat
    if (peers.viewed != null) {
        peers.Profile(opened, context.section, feed, open)
    } else if (chat != null) {
        CompositionLocalProvider(
            LocalSharedProfile provides { peer ->
                peers.found = null
                peers.viewed = peer
            }
        ) {
            ChatForwardScope(feed, chat, { target -> peers.chat = target }) {
                ChatScreen(opened, chat, feed, { peers.viewed = chat }) { peers.chat = null }
            }
        }
    } else {
        ShellSections(context, feed, search, peers)
    }
}

@Composable
private fun ShellSections(context: ShellContext, feed: ChatFeed, search: SearchSession, peers: ShellPeers) {
    val openChat = { peer: String ->
        feed.lists.blocked = null
        peers.chat = peer
    }
    CompositionLocalProvider(LocalSharedProfile provides peers::showSaved) {
        ShellFrame(context, feed, ShellChatActions(openChat, peers::show)) {
            SearchScreen(search, { context.controls.open(StartTarget.NETWORK) }, peers::show, peers::showSaved)
        }
    }
}

@Composable
private fun ShellFrame(
    context: ShellContext,
    feed: ChatFeed,
    chat: ShellChatActions,
    search: @Composable () -> Unit,
) {
    var switcher by remember { mutableStateOf(false) }
    val current = sections.firstOrNull { it.target == context.section } ?: sections.first()
    val detail = current.target == StartTarget.SETTINGS && LocalSettingsPane.current?.value != SettingsPane.ROOT
    Scaffold(
        topBar = { if (!detail) ShellTopBar(context.opened, current, context.controls.open) { switcher = true } },
        bottomBar = { if (!detail) ShellNavigationBar(current, chatsBadge(feed.rows), context.controls.open) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            if (current.target == StartTarget.CHATS) {
                ChatsScreen(feed, chat.open, chat.found)
            } else {
                SectionContent(context, current.target, search)
            }
        }
    }
    if (switcher) ShellIdentitySheet(context) { switcher = false }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ShellIdentitySheet(context: ShellContext, dismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = dismiss, Modifier.testTag("identity-menu")) {
        IdentityMenu(context.slots, context.opened) { action ->
            dismiss()
            context.controls.act(action)
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
            IconButton(
                onClick = onSwitch,
                Modifier.testTag("identity-switcher").semantics { contentDescription = identities }
            ) {
                Avatar(opened.peerId, rememberOwnPhoto(opened))
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
private fun SectionContent(context: ShellContext, target: StartTarget, search: @Composable () -> Unit) {
    val opened = context.opened
    val controls = context.controls
    when (target) {
        StartTarget.SEARCH -> search()
        StartTarget.NETWORK -> NetworkDraftScreen(opened, null)
        StartTarget.SETTINGS -> IdentitySettingsScreen(
            context.slots,
            opened,
            controls.lockMode,
            IdentitySettingsActions(controls.act) { controls.open(StartTarget.LOCK) }
        )
        else -> Column {
            Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp)) {
                Text(stringResource(R.string.storage_peer_id), style = MaterialTheme.typography.labelMedium)
                Text(opened.peerId, Modifier.testTag("peer-id"))
                // The OS gives the app no chance to warn before these erase the key.
                Text(
                    stringResource(R.string.storage_os_loss),
                    Modifier.testTag("os-loss"),
                    style = MaterialTheme.typography.bodySmall
                )
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
    AssistChip(
        onClick = onClick,
        modifier = Modifier.testTag("connection-status"),
        label = { Text(name?.ifEmpty { public } ?: stringResource(R.string.status_no_network)) }
    )
}

// No photo yet: the first two Peer ID characters on a tonal circle.
@Composable
internal fun Avatar(peerId: String, photo: ImageBitmap? = null) {
    Box(
        Modifier.size(32.dp).clip(CircleShape).background(MaterialTheme.colorScheme.secondaryContainer),
        contentAlignment = Alignment.Center,
    ) {
        // Decorative: the row next to it names the person.
        if (photo != null) {
            Image(photo, null, Modifier.fillMaxSize().testTag("avatar-photo"), contentScale = ContentScale.Crop)
        } else {
            Text(peerId.take(2), style = MaterialTheme.typography.labelMedium)
        }
    }
}
