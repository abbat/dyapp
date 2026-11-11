package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performSemanticsAction

internal fun ComposeTestRule.profileSaveRevision(): Int = runCatching {
    onNodeWithTag("profile-status").fetchSemanticsNode().config[profileSaveRevisionKey]
}.getOrDefault(0)

internal fun ComposeTestRule.beginProfileSave(): Int {
    awaitNode(hasTestTag("profile-save") and isEnabled())
    val previous = profileSaveRevision()
    onNodeWithTag("profile-save").performSemanticsAction(SemanticsActions.OnClick)
    return previous
}

internal fun ComposeTestRule.awaitProfileSave(previous: Int) {
    waitUntil(60_000) { profileSaveRevision() > previous }
    waitForIdle()
}

internal fun ComposeTestRule.saveProfile() = awaitProfileSave(beginProfileSave())
