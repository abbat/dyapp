package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission

class ProfileShareDialogTests {
    @get:Rule
    val compose = createComposeRule()
    private val form = ProfileShareForm().apply { shown = true }
    private val sent = mutableListOf<String>()
    private var opened by mutableStateOf(emptyList<String>())
    private val peer = "a".repeat(64)

    private fun string(id: Int) =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun show(rows: List<ChatRow> = emptyList()) = compose.setContent {
        Column {
            TextButton({ error("The background Message action was selected") }) {
                Text(stringResource(R.string.view_message))
            }
            opened.lastOrNull()?.let { Text(it, Modifier.testTag("opened-shared-chat")) }
        }
        if (form.shown) ProfileShareDialog(form, rows, { opened += it }) { sent += it }
    }

    private fun row() = ChatRow(
        peerId = peer,
        state = ChatState.OPEN,
        lastMessage = null,
        preview = ChatListPreview(ChatPreviewBody.Empty, false, null),
        unreadCount = 0uL,
        muted = false,
    )

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
        show(listOf(row()))
        compose.onNodeWithText(string(R.string.chat_send_failed)).assertExists()
        compose.onNodeWithTag("profile-share-$peer").performClick()
        compose.runOnIdle {
            assertEquals(listOf(peer), sent)
            val queued = ChatSubmission.Queued(byteArrayOf(2), "offline")
            form.completeSubmission(Result.success(queued), peer, form.opening)
        }
        compose.onNodeWithTag("profile-share-$peer").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.chat_queued)).assertExists()
        openSharedChat()
        compose.runOnIdle {
            assertEquals(listOf(peer), opened)
            assertFalse(form.shown)
        }
    }

    private fun openSharedChat() {
        compose.awaitNode(hasTestTag(PROFILE_SHARE_OPEN_CHAT_TAG)).assertIsDisplayed().performClick()
        compose.awaitNode(hasTestTag("opened-shared-chat"))
        compose.onNodeWithTag(PROFILE_SHARE_OPEN_CHAT_TAG).assertDoesNotExist()
    }

    @Test
    fun acceptedCommitmentOpensTheExactTargetAndClosesWithoutAnotherSend() {
        form.completeSubmission(Result.success(ChatSubmission.Accepted(byteArrayOf(1))), peer, form.opening)
        show(listOf(row()))
        openSharedChat()
        compose.runOnIdle {
            assertEquals(listOf(peer), opened)
            assertEquals(emptyList<String>(), sent)
            assertFalse(form.shown)
        }
    }

    @Test
    fun inFlightSendDisablesCancelAndHidesTargets() {
        form.sending = true
        show(listOf(row()))
        compose.onNodeWithText(string(R.string.search_cancel)).assertIsNotEnabled()
        compose.onNodeWithText(string(R.string.view_sending)).assertExists()
        compose.onNodeWithTag("profile-share-$peer").assertDoesNotExist()
    }
}
