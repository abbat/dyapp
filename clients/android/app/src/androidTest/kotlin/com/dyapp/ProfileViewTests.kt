package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatLikes
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.PublicProfileDraft

internal abstract class ProfileViewFixture {
    @get:Rule
    val compose = createComposeRule()

    protected val form = ProfileForm()
    protected val likes = mutableListOf<Boolean>()
    protected val passes = mutableListOf<Boolean>()
    protected var blocks = 0
    protected var backs = 0
    protected var messages = 0
    protected var retries = 0
    protected val found = FoundProfile(
        "peer-abcdef",
        PublicProfileDraft(
            0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u,
            birthYear = null, birthMonth = null, birthDay = null, country = "", place = ""
        ),
        emptyList(),
        emptyList(),
        1uL
    )

    protected fun string(id: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    protected fun show(load: ProfileLoad = ProfileLoad.Shown(found), own: Boolean = false) = compose.setContent {
        val actions = ProfileActions(R.string.view_back, { backs += 1 }, { messages += 1 })
        val onBlock = { blocks += 1 }
        ProfileViewContent("peer-abcdef", own, load, form, actions, { likes += it }, { passes += it }, onBlock) {
            retries += 1
        }
    }
}

internal class ProfileViewTests : ProfileViewFixture() {
    @Test
    fun aFoundProfileShowsItsFieldsAndTheFirstActions() {
        form.likes = ChatLikes(ours = false, matched = false, passed = false)
        show()
        compose.onNodeWithTag("view-fields").assertExists()
        compose.onNodeWithTag("view-peer").assertExists()
        compose.onNodeWithTag("view-unlike").assertDoesNotExist()
        compose.onNodeWithTag("view-match").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.view_first_hint)).assertExists()
        compose.onNodeWithTag("view-like").assertIsEnabled().performClick()
        compose.onNodeWithTag("view-message").performClick()
        compose.runOnIdle {
            assertEquals(listOf(true), likes)
            assertEquals(1, messages)
        }
    }

    @Test
    fun likesStillUnreadOrASendInFlightBlockTheButton() {
        show()
        compose.onNodeWithTag("view-like").assertIsNotEnabled()
        compose.runOnIdle {
            form.likes = ChatLikes(ours = false, matched = false, passed = false)
            form.sending = true
        }
        compose.onNodeWithTag("view-like").assertIsNotEnabled()
        compose.onNodeWithText(string(R.string.view_sending)).assertExists()
    }

    @Test
    fun aMatchOffersTheOpenMessageAndTakingTheLikeBackAsks() {
        form.likes = ChatLikes(ours = true, matched = true, passed = false)
        show()
        compose.onNodeWithTag("view-match").assertExists()
        compose.onNodeWithTag("view-message-match").assertExists()
        compose.onNodeWithTag("view-like").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.view_first_hint)).assertDoesNotExist()
        compose.onNodeWithTag("view-unlike").performClick()
        compose.onNodeWithTag("unlike-cancel").performClick()
        compose.onNodeWithTag("unlike-confirm").assertDoesNotExist()
        compose.runOnIdle { assertEquals(emptyList<Boolean>(), likes) }
        compose.onNodeWithTag("view-unlike").performClick()
        compose.onNodeWithTag("unlike-confirm").performClick()
        compose.runOnIdle { assertEquals(listOf(false), likes) }
    }

    @Test
    fun aLikeWithoutAMatchStillHasTheOneMessageRule() {
        form.likes = ChatLikes(ours = true, matched = false, passed = false)
        show()
        compose.onNodeWithTag("view-unlike").assertExists()
        compose.onNodeWithTag("view-message").assertExists()
        compose.onNodeWithText(string(R.string.view_first_hint)).assertExists()
        compose.onNodeWithText(string(R.string.view_unlike_hint)).assertExists()
    }

    @Test
    fun theOwnProfileHasNothingToLikeOrWriteTo() {
        form.likes = ChatLikes(ours = false, matched = false, passed = false)
        show(own = true)
        compose.onNodeWithTag("view-fields").assertExists()
        compose.onNodeWithTag("view-like").assertDoesNotExist()
        compose.onNodeWithTag("view-message").assertDoesNotExist()
        compose.onNodeWithTag("view-pass").assertDoesNotExist()
        compose.onNodeWithTag("view-block").assertDoesNotExist()
    }

    @Test
    fun aFailedLikeShowsItsReason() {
        form.likes = ChatLikes(ours = false, matched = false, passed = false)
        form.status = ChatStatus(R.string.view_like_unpublished, "details")
        show()
        compose.onNodeWithText(string(R.string.view_like_unpublished)).assertExists()
    }

    @Test
    fun aStagedLikeSaysItGoesOutLater() {
        form.likes = ChatLikes(ours = true, matched = false, passed = false)
        form.info = R.string.view_like_pending
        show()
        compose.onNodeWithText(string(R.string.view_like_pending)).assertExists()
    }

    @Test
    fun passIsOfferedOnlyWithoutALikeAndHidesLikeUntilUndone() {
        form.likes = ChatLikes(ours = true, matched = false, passed = false)
        show()
        compose.onNodeWithTag("view-pass").assertDoesNotExist()
        compose.runOnIdle { form.likes = ChatLikes(ours = false, matched = false, passed = false) }
        compose.onNodeWithTag("view-unpass").assertDoesNotExist()
        compose.onNodeWithTag("view-pass").performScrollTo().performClick()
        compose.runOnIdle {
            form.likes = ChatLikes(ours = false, matched = false, passed = true)
            form.info = R.string.view_passed
        }
        compose.onNodeWithTag("view-like").assertDoesNotExist()
        compose.onNodeWithTag("view-pass").assertDoesNotExist()
        compose.onNodeWithText(string(R.string.view_passed)).assertExists()
        compose.onNodeWithTag("view-unpass").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(listOf(true, false), passes) }
    }

    @Test
    fun blockAsksFirstAndCancelKeepsThePage() {
        form.likes = ChatLikes(ours = false, matched = false, passed = false)
        show()
        compose.onNodeWithTag("view-block").performScrollTo().performClick()
        compose.onNodeWithTag("block-cancel").performClick()
        compose.onNodeWithTag("block-dialog").assertDoesNotExist()
        compose.runOnIdle { assertEquals(0, blocks) }
        compose.onNodeWithTag("view-block").performScrollTo().performClick()
        compose.onNodeWithTag("block-confirm").performClick()
        compose.runOnIdle { assertEquals(1, blocks) }
    }
}

internal class ProfileLoadViewTests : ProfileViewFixture() {
    @Test
    fun aProfileNotFetchedOffersARetry() {
        show(ProfileLoad.Missing)
        compose.onNodeWithText(string(R.string.view_not_found)).assertExists()
        compose.onNodeWithTag("view-fields").assertDoesNotExist()
        compose.onNodeWithTag("view-retry").performClick()
        compose.runOnIdle { assertEquals(1, retries) }
    }

    @Test
    fun aFailedFetchShowsItsReasonAndARetry() {
        show(ProfileLoad.Failed(ChatStatus(R.string.view_load_failed, "details")))
        compose.onNodeWithText(string(R.string.view_load_failed)).assertExists()
        compose.onNodeWithTag("view-retry").assertExists()
    }

    @Test
    fun theBackButtonLeavesThePageAndNamesWhere() {
        show(ProfileLoad.Loading)
        compose.onNodeWithText(string(R.string.view_loading)).assertExists()
        compose.onNodeWithTag("view-back").performClick()
        compose.runOnIdle { assertEquals(1, backs) }
    }
}
