package com.dyapp

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState
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

    private fun openCardInHistory(author: String) {
        val opened = mutableListOf<String>()
        val link = checkNotNull(profileLinkText(peer))
        val messages = listOf(message(author, link)) + List(20) { index ->
            message("conversation", "later $index").copy(id = byteArrayOf((index + 2).toByte()))
        }
        val history = ChatHistory(ChatState.OPEN, messages, false)
        val form = ChatForm(mutableStateOf(""))
        val actions = ChatActions({}, {}, {}, {}, {})
        compose.setContent {
            CompositionLocalProvider(LocalSharedProfile provides { opened += it }) {
                ChatContent("conversation", "me", history, form, actions)
            }
        }
        compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
        compose.onNodeWithTag("chat-thread").performScrollToNode(hasTestTag("shared-profile-$peer"))
        compose.onNodeWithTag("shared-profile-$peer").performClick()
        compose.runOnIdle { assertEquals(listOf(peer), opened) }
    }

    @Test
    fun anIncomingCardRemainsReachableAfterScrollingToNewerMessages() = openCardInHistory("conversation")

    @Test
    fun anOwnCardRemainsReachableInsideTheMessageMenuBubble() = openCardInHistory("me")

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
