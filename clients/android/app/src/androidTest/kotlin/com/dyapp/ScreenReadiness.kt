package com.dyapp

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo

/** Each transition gets a fresh failure deadline and observes its actual semantic completion. */
internal fun ComposeTestRule.awaitNode(matcher: SemanticsMatcher): SemanticsNodeInteraction {
    waitUntil(60_000) { onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty() }
    return onNode(matcher)
}

/** Return through at most the collection/category levels, observing each completed transition. */
internal fun ComposeTestRule.settingsRoot() {
    repeat(2) {
        if (onAllNodesWithTag("settings-back").fetchSemanticsNodes().isNotEmpty()) {
            val parents = mapOf(
                "settings-identities-screen" to "settings-account",
                "settings-blocked" to "settings-privacy",
                "settings-passed" to "settings-privacy"
            )
            val parent = parents.entries.firstOrNull {
                onAllNodesWithTag(it.key).fetchSemanticsNodes().isNotEmpty()
            }?.value ?: "settings-root"
            onNodeWithTag("settings-back").performClick()
            awaitNode(hasTestTag(parent))
        }
    }
}

internal fun ComposeTestRule.openSettingsPane(vararg tags: String) {
    settingsRoot()
    awaitNode(hasTestTag("nav-settings")).performClick()
    // Entering the tab can restore its detail route after Activity recreation.
    awaitNode(hasTestTag("settings-root") or hasTestTag("settings-back"))
    settingsRoot()
    tags.forEach {
        awaitNode(hasTestTag("open-" + it)).performScrollTo().performClick()
        awaitNode(hasTestTag(it))
    }
}

internal fun ComposeTestRule.openProfileEditor() {
    settingsRoot()
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
