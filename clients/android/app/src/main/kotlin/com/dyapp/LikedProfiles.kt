package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.FoundProfile

/** The failure from fetching the liked-you profiles. */
@Composable
internal fun LikedFailure(failure: Throwable) {
    val status = profileLoadStatus(failure)
    ErrorNotice(
        stringResource(status.text),
        "chats-liked-status",
        status.details.orEmpty(),
        "chats-liked-details",
        "copy-chats-liked-details"
    )
}

/** The fetch can be older than the IDs, so a peer who left the list leaves at once. */
@Composable
internal fun LikedProfiles(liked: List<String>, found: List<FoundProfile>, open: (FoundProfile) -> Unit) {
    val shown = found.filter { it.peerId in liked }
    FetchCardPhotos(shown)
    for ((index, profile) in shown.withIndex()) {
        SearchResultRow(profile, "liked-${index + 1}") { open(profile) }
    }
    if (shown.size < liked.size) Caption(stringResource(R.string.chats_liked_missing))
}
