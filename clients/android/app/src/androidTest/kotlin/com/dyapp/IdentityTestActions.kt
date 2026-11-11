package com.dyapp

import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithTag

/** Actual completed-action revision; no assumption that an intermediate loading frame was drawn. */
internal fun ComposeTestRule.identityRevision(): Long = runCatching {
    onNodeWithTag("identity-state", useUnmergedTree = true).fetchSemanticsNode().config[identityReadyRevisionKey]
}.getOrDefault(0L)

/** A fresh failure deadline for this transition, including results that show the same screen. */
internal fun ComposeTestRule.awaitIdentityCompletion(previous: Long, timeoutMillis: Long = 60_000) {
    waitUntil(timeoutMillis) { identityRevision() > previous }
    waitForIdle()
}

/** Perform a ready identity command and await its own completion rather than an older matching view. */
internal fun ComposeTestRule.identityAction(timeoutMillis: Long = 60_000, action: () -> Unit) {
    waitUntil(timeoutMillis) { identityRevision() > 0 }
    val previous = identityRevision()
    action()
    awaitIdentityCompletion(previous, timeoutMillis)
}
