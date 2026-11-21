package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

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

private fun ComposeTestRule.taggedBounds() = runCatching {
    onAllNodes(
        SemanticsMatcher("tagged") { SemanticsProperties.TestTag in it.config },
        useUnmergedTree = true
    ).fetchSemanticsNodes().take(40).map { "${it.config[SemanticsProperties.TestTag]}:${it.boundsInRoot}" }
}.getOrNull()

internal fun ComposeTestRule.awaitOpenChatSnapshot() {
    val thread = awaitNode(hasTestTag("chat-thread") and SemanticsMatcher.keyIsDefined(chatHistoryStateKey))
    val state = thread.fetchSemanticsNode().config[chatHistoryStateKey]
    check(state == uniffi.dyapp.ChatState.OPEN) { "Chat snapshot state is $state" }
    awaitNode(hasTestTag("chat-input") and isEnabled())
}

private fun ownProcessChatThreadStacks(): String {
    // Taking another native admission could block the failure diagnostic itself.
    val relevant = { frame: StackTraceElement ->
        frame.className.startsWith("com.dyapp.") || frame.className.startsWith("uniffi.dyapp.")
    }
    val threads = Thread.getAllStackTraces().entries.filter { (_, frames) ->
        frames.any(relevant)
    }.sortedBy { it.key.name }
    val summary = threads.take(16).joinToString { (thread, frames) ->
        "${thread.name}:${thread.state} " + frames.filter(relevant).take(8).joinToString()
    }
    return "threads=${threads.size} showing=${minOf(16, threads.size)} $summary"
}

private fun ComposeTestRule.reopenSharedCard(peer: TestPeer, me: String, card: SemanticsMatcher) {
    var stage = "share destination action"
    runCatching {
        awaitNode(hasTestTag(PROFILE_SHARE_OPEN_CHAT_TAG) and isEnabled()).assertIsDisplayed().performClick()
        stage = "chat route"
        awaitNode(hasTestTag("chat"))
        stage = "loaded chat"
        // Observe the completed snapshot before testing its separate composing policy.
        awaitOpenChatSnapshot()
        stage = "shared card in loaded chat"
        onNodeWithTag("chat-thread").performScrollToNode(card)
        // Both messages contain this link; reveal the currently composed match before tapping.
        onAllNodes(card).onFirst().performScrollTo().performClick()
        awaitNode(hasTestTag("view-share"))
    }.onFailure { failure ->
        val thread = runCatching { textsIn("chat-thread").take(8) }.getOrNull()
        val status = runCatching { texts("chat-status") }.getOrNull()
        val received = runCatching {
            peer.session.chatHistory(me).messages
                .filter { it.author == me }
                .map { it.text?.take(100) }
                .takeLast(8)
        }.getOrNull()
        throw AssertionError(
            "Shared card $stage: tags=${taggedBounds()}, thread=$thread, status=$status, delivered=$received; " +
                ownProcessChatThreadStacks(),
            failure
        )
    }
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
                // Navigation renders before the IO history load enables the composer.
                compose.awaitOpenChatSnapshot()
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
        compose.reopenSharedCard(peer, me, card)
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
