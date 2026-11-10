package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile

/** The current owner's verified profile or its loading/failure state. */
internal sealed interface ProfileLoad {
    data object Loading : ProfileLoad
    data class Shown(val found: FoundProfile) : ProfileLoad
    data object Missing : ProfileLoad
    data class Failed(val status: ChatStatus) : ProfileLoad
}

@Composable
internal fun rememberProfileLoad(peer: String, known: FoundProfile?, feed: ChatFeed, attempt: Int): State<ProfileLoad> {
    // The searcher also updates the verified profile/photo cache.
    val photos = LocalPeerPhotos.current
    return feed.rememberProfileResponse(peer, known, attempt) {
        withContext(Dispatchers.IO) {
            (photos?.profiles(listOf(peer)) ?: feed.profiles(listOf(peer))).firstOrNull()
        }
    }
}

/** Retry preserves a shown profile; changing the owner resets even an already-shown value. */
@Composable
internal fun ChatFeed.rememberProfileResponse(
    peer: String,
    known: FoundProfile?,
    attempt: Int,
    fetch: suspend () -> FoundProfile?,
): State<ProfileLoad> {
    val current = known?.takeIf { it.peerId == peer }
    return key(peer, this, current) {
        produceState(current?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Loading, attempt) {
            if (value !is ProfileLoad.Shown) {
                value = ProfileLoad.Loading
                val fetched = runCatching { fetch() }
                // A provider can return after cancellation; do not publish its superseded result.
                currentCoroutineContext().ensureActive()
                value = fetched.fold(
                    { found -> found?.let { ProfileLoad.Shown(it) } ?: ProfileLoad.Missing },
                    { ProfileLoad.Failed(profileLoadStatus(it)) },
                )
            }
        }
    }
}
