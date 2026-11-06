package com.dyapp

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

// The foreground receive runs once a minute, so a peer's text can take two rounds to show.
internal const val RECEIVE_WAIT_MS = 150_000L

internal fun ComposeTestRule.shown(matcher: SemanticsMatcher) =
    onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

/** Waits for [text] in the open chat's thread. */
internal fun ComposeTestRule.waitForThreadText(text: String) = runCatching {
    waitUntil(RECEIVE_WAIT_MS) { shown(hasText(text) and hasAnyAncestor(hasTestTag("chat-thread"))) }
}.onFailure { throw AssertionError("chat-thread shows ${textsIn("chat-thread")}", it) }

/** The Peer ID of this app's open identity. */
internal fun ownPeerId(): String {
    val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)
    return (slots.open() as Opened.Ready).peerId
}

/** Two identities on the runner's node: this app's and a [TestPeer] the test drives through the FFI. */
class TwoPeerChatTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    @Test
    fun aRequestOpensFromItsRowAndTheReplyReachesThePeer() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.publishToReadback()
            val me = ownPeerId()
            TestPeer("request").use { peer ->
                peer.session.sendText(me, "Hello from the peer")
                compose.onNodeWithTag("nav-chats").performClick()
                compose.onNodeWithTag("chats-requests").performClick()
                // Rows name the short Peer ID; an earlier test can leave other rows for this identity.
                val row = hasText(shortPeerId(peer.peerId)) and hasAnyAncestor(hasTestTag("chats-requests-list"))
                runCatching { compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(row) } }
                    .onFailure { throw AssertionError("Requests show ${compose.textsIn("chats-requests-list")}", it) }
                compose.onNode(row).performClick()
                // The chat state loads off the main thread, which waitForIdle does not cover.
                compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(hasTestTag("chat-request-notice")) }
                compose.waitForThreadText("Hello from the peer")

                compose.onNodeWithTag("chat-input").performTextInput("Hello back")
                compose.onNodeWithTag("chat-send").performClick()
                compose.waitForThreadText("Hello back")
                peer.awaitMessage(me, "the reply") { it.text == "Hello back" }
                shareOwnProfile(peer, me)
                openUnavailableProfile(peer, me)
            }
        } finally {
            forgetNetworks()
        }
    }

    @Test
    fun aMatchFromLikedYouOpensTheChatWhereEditDeleteAndBlockReachThePeer() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.publishToReadback()
            val me = ownPeerId()
            TestPeer("match").use { peer ->
                peer.session.like(me, true)
                matchFromLikedYou(peer)
                compose.onNodeWithTag("view-message-match").performClick()
                compose.onNodeWithTag("chat-input").performTextInput("First draft")
                compose.onNodeWithTag("chat-send").performClick()
                compose.waitForThreadText("First draft")
                peer.awaitMessage(me, "the text") { it.text == "First draft" }
                editAndDelete(peer, me)
                block()
            }
        } finally {
            forgetNetworks()
        }
    }

    @Test
    fun aLikedProfileShowsThePeersPhoto() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.publishToReadback()
            val me = ownPeerId()
            TestPeer("photo", photo = true).use { peer ->
                peer.session.like(me, true)
                openLiked(peer)
                // The description is on the drawn photo only; the letters placeholder has none.
                val label = compose.activity.getString(R.string.profile_photo_label, 1, 1)
                val photo = hasContentDescription(label) and hasAnyAncestor(hasTestTag("view-photo"))
                runCatching { compose.waitUntil(CHECK_WAIT_MS) { compose.shown(photo) } }
                    .onFailure { throw AssertionError("view-photo shows ${compose.textsIn("view-photo")}", it) }
            }
        } finally {
            forgetNetworks()
        }
    }

    private fun openUnavailableProfile(peer: TestPeer, me: String) {
        val missing = "b".repeat(64)
        peer.session.sendText(me, checkNotNull(uniffi.dyapp.profileLinkText(missing)))
        val card = hasTestTag("shared-profile-$missing")
        compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(card) }
        compose.onNode(card).performClick()
        val unavailable = compose.activity.getString(R.string.view_not_found)
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasText(unavailable)) }
        compose.onNodeWithTag("view-retry").assertExists()
        compose.onNodeWithTag("view-back").performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("chat-thread")) }
    }

    private fun shareOwnProfile(peer: TestPeer, me: String) {
        val link = checkNotNull(uniffi.dyapp.profileLinkText(me))
        peer.session.sendText(me, link)
        val card = hasTestTag("shared-profile-$me")
        compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(card) }
        compose.onNode(card).performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("view-share")) }
        compose.onNodeWithTag("view-share").performScrollTo().performClick()
        compose.onNodeWithTag("profile-share-${peer.peerId}").performClick()
        peer.awaitMessage(me, "the shared profile") { it.text == link }
        val message = compose.activity.getString(R.string.view_message)
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasText(message)) }
        compose.onNodeWithText(message).performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("chat-thread")) }
        // Both messages contain the link; either card must open the same verified profile.
        compose.onAllNodes(card)[0].performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("view-share")) }
        compose.onNodeWithTag("view-back").performClick()
        compose.waitForThreadText("Hello back")
    }

    // The like back from the peer's page makes the match.
    private fun matchFromLikedYou(peer: TestPeer) {
        openLiked(peer)
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("view-like") and isEnabled()) }
        compose.onNodeWithTag("view-like").performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("view-match")) }
    }

    // Liked you lists the peer once a receive brings the like; its row opens the peer's page.
    private fun openLiked(peer: TestPeer) {
        compose.onNodeWithTag("nav-chats").performClick()
        compose.onNodeWithTag("chats-liked").performClick()
        val row = hasText(shortPeerId(peer.peerId)) and hasAnyAncestor(hasTestTag("chats-liked-list"))
        runCatching { compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(row) } }
            .onFailure { throw AssertionError("Liked you shows ${compose.textsIn("chats-liked-list")}", it) }
        compose.onNode(row).performClick()
    }

    private fun press(text: String) =
        compose.onNode(hasText(text) and hasAnyAncestor(hasTestTag("chat-thread"))).performTouchInput { longClick() }

    private fun editAndDelete(peer: TestPeer, me: String) {
        press("First draft")
        compose.onNodeWithTag("msg-edit").performClick()
        compose.onNodeWithTag("chat-input").performTextReplacement("Edited text")
        compose.onNodeWithTag("chat-send").performClick()
        compose.waitForThreadText("Edited text")
        peer.awaitMessage(me, "the edit") { it.edited && it.text == "Edited text" }
        press("Edited text")
        compose.onNodeWithTag("msg-delete").performClick()
        compose.onNodeWithTag("delete-confirm").performClick()
        compose.waitForThreadText(compose.activity.getString(R.string.chats_deleted))
        peer.awaitMessage(me, "the delete") { it.deleted }
    }

    // Blocking leaves the chat; the note on Chats says how the revocation went.
    private fun block() {
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-block").performClick()
        compose.onNodeWithTag("block-confirm").performClick()
        compose.waitUntil(CHECK_WAIT_MS) { compose.shown(hasTestTag("chats-blocked-note")) }
    }
}
