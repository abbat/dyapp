package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatBlock
import uniffi.dyapp.ChatRevocation
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.PublicProfileDraft

class ChatsScreenTests {
    @get:Rule
    val compose = createComposeRule()

    private val lists = ChatsLists()
    private val opened = mutableListOf<String>()
    private val profiles = mutableListOf<String>()

    private fun string(id: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun row(peer: String, state: ChatState) = ChatRow(peer, state, null, 0uL, false)

    private fun found(peer: String) = FoundProfile(
        peer,
        PublicProfileDraft(
            gender = 0u,
            orientation = 0u,
            relationshipGoal = 0u,
            rolePreference = 0u,
            careerAmbition = 0u,
            financialPhilosophy = 0u,
            employmentStatus = 0u,
            fitnessLevel = 0u,
            zodiac = 0u,
            birthYear = null,
            birthMonth = null,
            birthDay = null,
            country = "",
            place = "",
        ),
        emptyList(),
        emptyList(),
        1uL
    )

    private fun show(rows: List<ChatRow> = emptyList(), liked: List<String> = emptyList()) = compose.setContent {
        ChatsContent(rows, liked, lists, { opened += it }) { profiles += it.peerId }
    }

    private fun count(tag: String, value: String) =
        compose.onNodeWithTag("$tag-count", useUnmergedTree = true).assertTextEquals(value)

    @Test
    fun aBlockSaysItsOutcome() {
        lists.blocked = ChatBlock(ChatRevocation.CONFIRMED, null)
        show()
        compose.onNodeWithTag("chats-blocked-note").assertTextEquals(string(R.string.chats_blocked_confirmed))
        compose.runOnIdle { lists.blocked = ChatBlock(ChatRevocation.PENDING, "notConnected") }
        compose.onNodeWithTag("chats-blocked-note").assertTextEquals(string(R.string.chats_blocked_pending))
        compose.onNodeWithText(string(R.string.details)).performClick()
        compose.onNodeWithTag("chats-blocked-details").assertTextEquals("notConnected")
        compose.runOnIdle { lists.blocked = null }
        compose.onNodeWithTag("chats-blocked-note").assertDoesNotExist()
    }

    @Test
    fun bothRowsShowLocalCountsAndStayClosedUntilOpened() {
        show(listOf(row("peer-request", ChatState.REQUEST_RECEIVED)), listOf("peer-a", "peer-b"))
        count("chats-liked", "2")
        count("chats-requests", "1")
        compose.onNodeWithTag("chats-liked-list").assertDoesNotExist()
        compose.onNodeWithTag("chats-requests-list").assertDoesNotExist()
    }

    @Test
    fun aRequestLeavesTheChatListAndOpensTheChat() {
        show(listOf(row("peer-request", ChatState.REQUEST_RECEIVED), row("peer-match", ChatState.OPEN)))
        compose.onNodeWithTag("chat-2").assertDoesNotExist()
        compose.onNodeWithTag("chat-1").performClick()
        assertEquals(listOf("peer-match"), opened)
        compose.onNodeWithTag("chats-requests").performClick()
        compose.onNodeWithTag("request-1").performClick()
        assertEquals(listOf("peer-match", "peer-request"), opened)
        compose.onNodeWithTag("chats-requests").performClick()
        compose.onNodeWithTag("chats-requests-list").assertDoesNotExist()
    }

    @Test
    fun onlyRequestsLeaveTheConversationsEmpty() {
        show(listOf(row("peer-request", ChatState.REQUEST_RECEIVED)))
        compose.onNodeWithTag("chat-list").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.chats_empty)).assertExists()
    }

    @Test
    fun emptyListsSayWhyWhenOpened() {
        show()
        compose.onNodeWithTag("chats-liked").performClick()
        compose.onNodeWithText(string(R.string.chats_none)).assertExists()
        compose.onNodeWithTag("chats-requests").performClick()
        compose.onNodeWithText(string(R.string.chats_no_requests)).assertExists()
    }

    @Test
    fun likedShowsLoadingThenOpensAFoundProfile() {
        show(liked = listOf("peer-a"))
        compose.onNodeWithTag("chats-liked").performClick()
        compose.onNodeWithText(string(R.string.view_loading)).assertExists()
        lists.likedProfiles = Result.success(listOf(found("peer-a")))
        compose.onNodeWithTag("liked-1").performClick()
        assertEquals(listOf("peer-a"), profiles)
    }

    @Test
    fun aPeerNoNodeReturnedCountsButHasNoRow() {
        lists.likedOpen = true
        lists.likedProfiles = Result.success(listOf(found("peer-a"), found("peer-left")))
        show(liked = listOf("peer-a", "peer-b"))
        count("chats-liked", "2")
        compose.onNodeWithTag("liked-1").assertExists()
        // A peer no longer in the local list goes even though the last fetch returned it.
        compose.onNodeWithTag("liked-2").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.chats_liked_missing)).assertExists()
    }

    @Test
    fun aFailedFetchShowsTheReasonAndDetails() {
        lists.likedOpen = true
        lists.likedProfiles = Result.failure(IllegalStateException("notConnected"))
        show(liked = listOf("peer-a"))
        compose.onNodeWithTag("chats-liked-status").assertTextEquals(string(R.string.search_not_connected))
        compose.onNodeWithText(string(R.string.details)).performClick()
        compose.onNodeWithTag("copy-chats-liked-details").assertExists()
    }
}
