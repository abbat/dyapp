package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.profileLinkPeer

/** Routing only: rendering a link must never fetch the peer's profile or photos. */
internal val LocalSharedProfile = staticCompositionLocalOf<(String) -> Unit> { {} }

internal fun sharedProfilePeer(message: ChatMessage): String? =
    if (message.deleted) null else message.text?.let(::profileLinkPeer)

@Composable
internal fun SharedProfileCard(peer: String) {
    val open = LocalSharedProfile.current
    TextButton({ open(peer) }, Modifier.testTag("shared-profile-$peer")) {
        Text(stringResource(R.string.shared_profile_card, shortPeerId(peer)))
    }
}
