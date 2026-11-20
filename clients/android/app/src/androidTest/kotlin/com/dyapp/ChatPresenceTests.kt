package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

class ChatPresenceTests {
    @get:Rule
    val compose = createComposeRule()

    private val presence = ChatPresence()
    private val row = ChatRow(
        peerId = "peer",
        state = ChatState.OPEN,
        lastMessage = null,
        preview = ChatListPreview(ChatPreviewBody.Empty, false, null),
        unreadCount = 0uL,
        muted = false
    )

    private fun mark() = compose.onNodeWithTag("chat-presence-peer", useUnmergedTree = true)

    private fun expected() =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.chat_active_recently)

    @Test
    fun rowTracksCoreRefreshAndHidesFailedReads() {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalActiveChatPeers provides presence.peers) {
                    ChatListRow(row, null, Modifier)
                }
            }
        }
        mark().assertDoesNotExist()
        compose.runOnIdle { presence.read { listOf("other") } }
        mark().assertDoesNotExist()
        compose.runOnIdle { presence.read { listOf("peer") } }
        mark().assertTextEquals(expected())
        compose.runOnIdle { presence.read { error("unavailable") } }
        mark().assertDoesNotExist()
        compose.runOnIdle { presence.read { listOf("peer") } }
        mark().assertTextEquals(expected())
        compose.runOnIdle { presence.read { emptyList() } }
        mark().assertDoesNotExist()
    }

    @Test
    fun openChatShowsOnlyTheCurrentOwnersProjection() {
        val owner = mutableStateOf(presence)
        val next = ChatPresence()
        val form = ChatForm(mutableStateOf(""))
        presence.read { listOf("peer") }
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalActiveChatPeers provides owner.value.peers) {
                    ChatContent(
                        "peer",
                        "me",
                        ChatHistory(ChatState.OPEN, emptyList(), false),
                        form,
                        ChatActions({}, {}, {}, {}, {})
                    )
                }
            }
        }
        mark().assertTextEquals(expected())
        compose.runOnIdle { owner.value = next }
        mark().assertDoesNotExist()
        compose.runOnIdle { presence.read { listOf("peer") } }
        mark().assertDoesNotExist()
        compose.runOnIdle { next.read { listOf("peer") } }
        mark().assertTextEquals(expected())
        compose.runOnIdle { next.read { error("owner no longer admitted") } }
        mark().assertDoesNotExist()
    }
}
