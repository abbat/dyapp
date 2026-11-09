package com.dyapp

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

/** A peer's photo and the local name on chat rows and the chat header, with a [TestPeer] on the runner's node. */
class ContactNameTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    // Android cannot publish a public name, so this covers the photo and a local name; Rust tests the rule.
    @Test
    fun aChatRowShowsThePeersPhotoAndALocalNameNamesAndClearsFromTheChatMenu() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.publishToReadback()
            val me = ownPeerId()
            TestPeer("named", photo = true).use { peer ->
                peer.session.sendText(me, "Hello with a photo")
                compose.onNodeWithTag("nav-chats").performClick()
                compose.onNodeWithTag("chats-requests").performClick()
                val short = shortPeerId(peer.peerId)
                runCatching { compose.waitUntil(RECEIVE_WAIT_MS) { compose.shown(requestRow(short)) } }
                    .onFailure { throw AssertionError("Requests show ${compose.textsIn("chats-requests-list")}", it) }
                waitUnmerged(hasTestTag("avatar-photo"), "chats-requests-list")
                renameFromRow(short, "Robin")
                renameFromRow("Robin", "")
                compose.waitUntil(CHECK_WAIT_MS) { compose.shown(requestRow(short)) }
            }
        } finally {
            forgetNetworks()
        }
    }

    private fun requestRow(label: String) = hasText(label) and hasAnyAncestor(hasTestTag("chats-requests-list"))

    // Opens the row labelled [label], names the peer [name] from the chat menu and goes back to the row.
    private fun renameFromRow(label: String, name: String) {
        compose.onNode(requestRow(label)).performClick()
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-rename").performClick()
        compose.onNodeWithTag("chat-rename-input").performTextReplacement(name)
        compose.onNodeWithTag("chat-rename-save").performClick()
        compose.waitUntil(CHECK_WAIT_MS) { !compose.shown(hasTestTag("chat-rename-dialog")) }
        if (name.isNotEmpty()) waitUnmerged(hasText(name), "chat-profile")
        compose.onNodeWithTag("chat-back").performClick()
        if (name.isNotEmpty()) compose.waitUntil(CHECK_WAIT_MS) { compose.shown(requestRow(name)) }
    }

    // A clickable row merges its avatar and title, so they are found in the unmerged tree.
    private fun waitUnmerged(matcher: SemanticsMatcher, tag: String) {
        val node = matcher and hasAnyAncestor(hasTestTag(tag))
        runCatching {
            compose.waitUntil(CHECK_WAIT_MS) {
                compose.onAllNodes(node, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
            }
        }.onFailure { throw AssertionError("$tag shows ${compose.textsIn(tag)}", it) }
    }
}
