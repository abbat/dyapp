package com.dyapp

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.profileLinkText

class SharedProfileCardTests {
    @get:Rule
    val compose = createComposeRule()
    private val peer = "a".repeat(64)

    @Test
    fun onlyAnExactLiveProfileLinkBecomesACard() {
        val link = checkNotNull(profileLinkText(peer))
        assertEquals(peer, sharedProfilePeer(message("sender", link)))
        assertNull(sharedProfilePeer(message("sender", "Look: $link")))
        assertNull(sharedProfilePeer(message("sender", peer)))
        assertNull(sharedProfilePeer(message("sender", link).copy(deleted = true)))
    }

    @Test
    fun renderingDoesNotOpenUntilPressed() {
        val opened = mutableListOf<String>()
        compose.setContent {
            CompositionLocalProvider(LocalSharedProfile provides { opened += it }) {
                SharedProfileCard(peer)
            }
        }
        compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
        compose.onNodeWithTag("shared-profile-$peer").performClick()
        compose.runOnIdle { assertEquals(listOf(peer), opened) }
    }
}
