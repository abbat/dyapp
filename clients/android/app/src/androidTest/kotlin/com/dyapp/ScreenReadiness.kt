package com.dyapp

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick

/** Each transition gets a fresh failure deadline and observes its actual semantic completion. */
internal fun ComposeTestRule.awaitNode(matcher: SemanticsMatcher): SemanticsNodeInteraction {
    waitUntil(60_000) { onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty() }
    return onNode(matcher)
}

internal fun ComposeTestRule.openProfileEditor() {
    onNodeWithTag("nav-profile").performClick()
    awaitNode(hasTestTag("profile-editor"))
}

internal fun ComposeTestRule.identityRows(expected: Int? = null): Int {
    waitUntil(60_000) {
        val size = onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size
        if (expected == null) size > 0 else size == expected
    }
    return onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size
}
