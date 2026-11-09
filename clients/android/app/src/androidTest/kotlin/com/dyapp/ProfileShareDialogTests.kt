package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

class ProfileShareDialogTests {
    @get:Rule
    val compose = createComposeRule()
    private val form = ProfileShareForm().apply { shown = true }
    private val sent = mutableListOf<String>()
    private val opened = mutableListOf<String>()
    private val peer = "a".repeat(64)

    private fun string(id: Int) =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun show(rows: List<ChatRow> = emptyList()) = compose.setContent {
        ProfileShareDialog(form, rows, { opened += it }) { sent += it }
    }

    @Test
    fun emptyTargetsExplainAndCancelSendsNothing() {
        show()
        compose.onNodeWithText(string(R.string.profile_share_empty)).assertExists()
        compose.onNodeWithText(string(R.string.search_cancel)).performClick()
        compose.runOnIdle {
            assertFalse(form.shown)
            assertEquals(emptyList<String>(), sent)
        }
    }

    @Test
    fun failedSendAllowsRetryAndSuccessOffersNavigation() {
        form.status = ChatStatus(R.string.chat_send_failed, "offline")
        show(listOf(ChatRow(peer, ChatState.OPEN, null, 0uL, false)))
        compose.onNodeWithText(string(R.string.chat_send_failed)).assertExists()
        compose.onNodeWithTag("profile-share-$peer").performClick()
        compose.runOnIdle {
            assertEquals(listOf(peer), sent)
            form.sentTo = peer
            form.status = ChatStatus(R.string.chat_queued, null)
        }
        compose.onNodeWithTag("profile-share-$peer").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.chat_queued)).assertExists()
        compose.onNodeWithText(string(R.string.view_message)).performClick()
        compose.runOnIdle {
            assertEquals(listOf(peer), opened)
            assertFalse(form.shown)
        }
    }

    @Test
    fun inFlightSendDisablesCancelAndHidesTargets() {
        form.sending = true
        show(listOf(ChatRow(peer, ChatState.OPEN, null, 0uL, false)))
        compose.onNodeWithText(string(R.string.search_cancel)).assertIsNotEnabled()
        compose.onNodeWithText(string(R.string.view_sending)).assertExists()
        compose.onNodeWithTag("profile-share-$peer").assertDoesNotExist()
    }
}
