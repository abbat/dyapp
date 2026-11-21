package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

private const val FORWARD_SOURCE_TEXT = "Real source words for native forwarding"

class NativeForwardUiTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    @Test
    fun realShellForwardsReceivedTextAndNativeReceiverUsesTheForwarderAsSender() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.publishToReadback()
            val owner = currentOwner()
            val conversations = NativeForwardConversations(owner)
            TestPeer("forward-source").use { source ->
                TestPeer("forward-target").use { target ->
                    conversations.establish(source, FORWARD_SOURCE_TEXT)
                    conversations.establish(target, "Destination invitation")
                    assertFalse(conversations.source(source, FORWARD_SOURCE_TEXT).forwarded)
                    openSource(source)
                    val current = NativeForwardConversations(currentOwner())
                    check(current.owner.peerId == owner.peerId) { "App owner changed on recreation" }
                    cancelBeforeForward(current, target)
                    forwardTo(target)
                    verifyDestination(current, target)
                }
            }
        } finally { forgetNetworks() }
    }

    private fun openSource(source: TestPeer) {
        compose.activityRule.scenario.recreate()
        compose.awaitNode(hasTestTag("app-ready"))
        compose.awaitNode(hasTestTag("nav-chats")).performClick()
        val row = hasText(shortPeerId(source.peerId)) and hasAnyAncestor(hasTestTag("chat-list"))
        compose.awaitNode(row).performScrollTo().performClick()
        compose.awaitOpenChatSnapshot()
        compose.waitForThreadText(FORWARD_SOURCE_TEXT)
    }

    private fun openPicker() {
        val source = hasText(FORWARD_SOURCE_TEXT) and hasAnyAncestor(hasTestTag("chat-thread"))
        compose.onNode(source).performTouchInput { longClick() }
        compose.onNodeWithTag("msg-forward").performClick()
        compose.awaitNode(hasTestTag("chat-forward-confirm") and isEnabled())
    }

    private fun cancelBeforeForward(conversations: NativeForwardConversations, target: TestPeer) {
        openPicker()
        compose.onNodeWithTag("chat-forward-cancel").performClick()
        compose.onNodeWithTag("chat-forward-dialog").assertDoesNotExist()
        assertFalse(conversations.history(target).messages.any { it.forwarded && it.text == FORWARD_SOURCE_TEXT })
    }

    private fun currentOwner(): Opened.Ready {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return RustBridge.slots(context).open() as Opened.Ready
    }

    private fun forwardTo(target: TestPeer) {
        openPicker()
        compose.onNodeWithTag("collection-query").performScrollTo().performTextReplacement(target.peerId)
        compose.awaitNode(hasTestTag("forward-target-" + target.peerId))
            .performScrollTo()
            .performSemanticsAction(SemanticsActions.OnClick)
        compose.onNodeWithTag("forward-selected-peer")
            .assertTextEquals(compose.activity.getString(R.string.search_peer, target.peerId))
        compose.onNodeWithTag("chat-forward-confirm")
            .assertIsEnabled()
            .performSemanticsAction(SemanticsActions.OnClick)
        awaitDestination(target)
        compose.awaitOpenChatSnapshot()
    }

    private fun awaitDestination(target: TestPeer) {
        val heading = hasText(shortPeerId(target.peerId)) and hasAnyAncestor(hasTestTag("chat-profile"))
        try {
            compose.waitUntil(60_000) {
                compose.onAllNodes(heading, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() ||
                    compose.shown(hasTestTag("forward-failed"))
            }
        } catch (failure: androidx.compose.ui.test.ComposeTimeoutException) {
            throw AssertionError("Forward did not complete: " + forwardState(), failure)
        }
        if (compose.shown(hasTestTag("forward-failed"))) {
            val details = hasText(compose.activity.getString(R.string.details)) and
                hasAnyAncestor(hasTestTag("chat-forward-dialog"))
            compose.onNode(details).performScrollTo().performClick()
            throw AssertionError("Native forward refused: " + forwardState())
        }
        compose.onNode(heading, useUnmergedTree = true).assertExists()
    }

    private fun forwardState() =
        "dialog=${compose.textsIn("chat-forward-dialog")}; header=${compose.textsIn("chat-profile", unmerged = true)}"

    private fun verifyDestination(conversations: NativeForwardConversations, target: TestPeer) {
        val forwarded = compose.activity.getString(R.string.chats_forwarded, FORWARD_SOURCE_TEXT)
        compose.waitForThreadText(forwarded)
        val received = conversations.delivered(target, FORWARD_SOURCE_TEXT)
        val receiver = compose.runOnIdle {
            NativeForwardReceiverView(compose.activity).apply {
                show(conversations.owner.peerId, target.peerId, received)
            }
        }
        try {
            compose.awaitNode(hasText(forwarded) and hasAnyAncestor(hasTestTag("native-forward-receiver")))
        } finally {
            compose.runOnIdle { receiver.close() }
        }
    }
}
