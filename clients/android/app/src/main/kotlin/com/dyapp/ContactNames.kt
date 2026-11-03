package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.ContactNameView

/**
 * Fetches the profiles of [peers] into the searcher's cache, then their main thumbnails, so their
 * public names and photos show; a failure keeps what shows, and the next view tries again.
 */
@Composable
internal fun FetchPeerProfiles(peers: List<String>) {
    val photos = LocalPeerPhotos.current ?: return
    LaunchedEffect(photos, peers) {
        if (peers.isEmpty()) return@LaunchedEffect
        val found = withContext(Dispatchers.IO) { runCatching { photos.profiles(peers) }.getOrNull() }
        photos.fetch(found.orEmpty().mapNotNull { it.photos.firstOrNull() }, false)
    }
}

/** How each of [peers] is named, read again after each fetch; a failed read keeps the last names. */
@Composable
internal fun rememberContactNames(peers: List<String>): Map<String, ContactNameView> {
    val photos = LocalPeerPhotos.current
    val names by produceState(emptyMap<String, ContactNameView>(), photos, peers, photos?.fetched) {
        val read = withContext(Dispatchers.IO) { runCatching { photos?.names(peers) }.getOrNull() }
        if (read != null) value = read.associateBy { it.peerId }
    }
    return names
}

/**
 * The desktop rule: a local name; else the public name, its mark and the short Peer ID, as a
 * public name is the peer's own claim; else the short Peer ID.
 */
@Composable
internal fun contactLabel(peer: String, view: ContactNameView?): String {
    val name = view?.name.orEmpty()
    val verified = view?.verified
    return when {
        name.isEmpty() -> shortPeerId(peer)
        verified == null -> name
        verified -> stringResource(R.string.contact_public_verified, name, shortPeerId(peer))
        else -> stringResource(R.string.contact_public_unverified, name, shortPeerId(peer))
    }
}

/** [peer]'s avatar, with the main photo once the cache has it. */
@Composable
internal fun PeerAvatar(peer: String, view: ContactNameView?) = Avatar(peer, rememberPeerPhoto(view?.photo))
