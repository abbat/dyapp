package com.dyapp

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

// The foreground receive runs once a minute, so a peer's text can take two rounds to show.
private const val RECEIVE_WAIT_MS = 150_000L

private fun ComposeTestRule.shown(matcher: SemanticsMatcher) =
    onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

/** Waits for [text] in the open chat's thread. */
internal fun ComposeTestRule.waitForThreadText(text: String) = runCatching {
    waitUntil(RECEIVE_WAIT_MS) { shown(hasText(text) and hasAnyAncestor(hasTestTag("chat-thread"))) }
}.onFailure { throw AssertionError("chat-thread shows ${textsIn("chat-thread")}", it) }

/** Two identities on the runner's node: this app's and a [TestPeer] the test drives through the FFI. */
class TwoPeerChatTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun ownPeerId(): String {
        val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)
        return (slots.open() as Opened.Ready).peerId
    }

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
                compose.onNodeWithTag("chat-request-notice").assertExists()
                compose.waitForThreadText("Hello from the peer")

                compose.onNodeWithTag("chat-input").performTextInput("Hello back")
                compose.onNodeWithTag("chat-send").performClick()
                compose.waitForThreadText("Hello back")
                peer.awaitMessage(me, "the reply") { it.text == "Hello back" }
            }
        } finally {
            forgetNetworks()
        }
    }
}
