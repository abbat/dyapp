package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

// The emulator runner's loopback dyappd, as the guest reaches its container.
internal const val LOCAL_NODE = "10.0.2.2:33445"
internal const val CHECK_WAIT_MS = 30_000L
private const val CHECK_TRIES = 3

private fun text(id: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

internal fun ComposeTestRule.texts(tag: String) = onAllNodes(hasTestTag(tag)).fetchSemanticsNodes()
    .map { it.config.getOrNull(SemanticsProperties.Text)?.joinToString() }

// A timeout names what the tag and the network check rows showed instead.
internal fun ComposeTestRule.waitForText(tag: String, id: Int) = runCatching {
    waitUntil(CHECK_WAIT_MS) { onAllNodes(hasTestTag(tag) and hasText(text(id))).fetchSemanticsNodes().isNotEmpty() }
}.onFailure { throw AssertionError("$tag shows ${texts(tag)}, rows ${texts("network-row")}", it) }

private fun ComposeTestRule.checkShows(vararg ids: Int) = ids.any { id ->
    onAllNodes(hasTestTag("network-check-status") and hasText(text(id))).fetchSemanticsNodes().isNotEmpty()
}

/** Runs the check until a result shows; with [retryUnreachable], retries as a user would. */
internal fun ComposeTestRule.checkNetwork(retryUnreachable: Boolean = false) {
    // The first dial of a cold, software-emulated app can outlast the 5 s check.
    repeat(if (retryUnreachable) CHECK_TRIES else 1) {
        if (checkShows(R.string.network_connected)) return@repeat
        onNodeWithTag("network-check").performScrollTo().performClick()
        waitUntil(CHECK_WAIT_MS) {
            checkShows(R.string.network_connected, R.string.network_unreachable, R.string.network_check_failed,
                R.string.network_invalid_address, R.string.network_wrong)
        }
    }
}

// Other tests expect no active network, so activated settings never outlive their test.
internal fun forgetNetworks() {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    for (root in listOf(context.filesDir, context.noBackupFilesDir)) {
        root.walkBottomUp().filter { it.isFile && it.name in setOf("network", "network-draft") }
            .forEach { it.delete() }
    }
}

class NetworkDraftTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    // The automatic form of an identity without an active network; the unlock takes seconds.
    private fun formPeerId(): String {
        compose.waitUntil(60_000) { compose.onAllNodesWithTag("network-back").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("app-ready").assertDoesNotExist()
        compose.waitForText("network-status", R.string.network_draft_only)
        return compose.texts("network-peer-id").single().orEmpty()
    }

    private fun enter(seeds: String, name: String) {
        compose.onNodeWithTag("network-seeds").performScrollTo().performTextClearance()
        compose.onNodeWithTag("network-seeds").performTextInput(seeds)
        compose.onNodeWithTag("network-name").performScrollTo().performTextClearance()
        if (name.isNotEmpty()) compose.onNodeWithTag("network-name").performTextInput(name)
    }

    private fun rows() = compose.texts("network-row")

    @Test
    fun savedDraftSurvivesRestartAndFailedRowsBlockContinue() {
        val peerId = formPeerId()
        val seeds = listOf("not an address", "10.0.2.2:1", "$LOCAL_NODE/$OTHER_PEER")
        try {
            enter(seeds.joinToString("\n"), "city")
            compose.onNodeWithTag("network-save").performScrollTo().performClick()
            compose.waitForText("network-status", R.string.network_saved)

            // A restart reopens the same identity on the automatic form with the saved draft.
            compose.activityRule.scenario.recreate()
            check(formPeerId() == peerId) { "Peer ID changed on restart" }
            compose.onNode(hasTestTag("network-seeds") and hasText(seeds.joinToString("\n"))).assertExists()
            compose.onNode(hasTestTag("network-name") and hasText("city")).assertExists()

            // An invalid row, a closed port and a wrong pin: nothing connects, Continue stays off.
            compose.checkNetwork()
            compose.waitForText("network-check-status", R.string.network_invalid_address)
            check(rows() == listOf(
                "${seeds[0]}: ${string(R.string.network_row_invalid_address)}",
                "${seeds[1]}: ${string(R.string.network_row_unreachable)}",
                "${seeds[2]}: ${string(R.string.network_row_wrong_peer)}",
            )) { "Unexpected rows ${rows()}" }
            compose.onNodeWithTag("network-continue").performScrollTo().assertIsNotEnabled()
            compose.onNodeWithTag("network-check").assertTextEquals(string(R.string.network_retry))

            // The live node under another network name.
            enter(LOCAL_NODE, "city")
            compose.onNodeWithTag("network-check").assertTextEquals(string(R.string.network_check))
            compose.checkNetwork()
            compose.waitForText("network-check-status", R.string.network_wrong)
            check(rows() == listOf("$LOCAL_NODE: ${string(R.string.network_row_wrong_network)}")) {
                "Unexpected rows ${rows()}"
            }
            compose.onNodeWithTag("network-continue").performScrollTo().assertIsNotEnabled()
        } finally {
            forgetNetworks()
        }
    }

    @Test
    fun retryAndContinueActivateTheNodeAndKeepTheIdentity() {
        val peerId = formPeerId()
        try {
            // Nothing listens on port 1, so the first check fails and the retry finds the node.
            enter("10.0.2.2:1", "")
            compose.checkNetwork()
            compose.waitForText("network-check-status", R.string.network_unreachable)
            compose.onNodeWithTag("network-continue").performScrollTo().assertIsNotEnabled()
            enter(LOCAL_NODE, "")
            compose.checkNetwork(retryUnreachable = true)
            compose.waitForText("network-check-status", R.string.network_connected)
            compose.onNodeWithTag("network-continue").performScrollTo().assertIsEnabled().performClick()
            compose.waitForText("network-check-status", R.string.network_active)

            // With an active network a restart lands on Ready with the same identity key.
            compose.activityRule.scenario.recreate()
            compose.waitUntil(60_000) { compose.onAllNodesWithTag("app-ready").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("network-back").assertDoesNotExist()
            check(compose.shownPeerId() == peerId) { "Peer ID changed after activation" }
        } finally {
            forgetNetworks()
        }
    }

    private companion object {
        // A well-formed Peer ID that no node holds.
        const val OTHER_PEER = "12D3KooW9tJMax94Lrqw7Y5Qw36viGQAS2gTEPQ5Wg1vTk7xPfQs"
    }
}
