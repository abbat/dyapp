package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag

private fun assertSendingDisabled(compose: ComposeContentTestRule) {
    compose.onNodeWithTag("owner-cancel").assertIsNotEnabled()
    compose.onNodeWithTag("owner-send").assertIsNotEnabled()
}

internal fun checkOwnerVisualState(compose: ComposeContentTestRule, state: OwnerVisualState) {
    for (tag in state.facts) compose.awaitNode(hasTestTag(tag))
    when (state) {
        OwnerVisualState.SENDING -> assertSendingDisabled(compose)
        OwnerVisualState.CONFIRM_EXPIRED -> compose.onNodeWithTag("owner-send").assertIsNotEnabled()
        OwnerVisualState.UNCERTAIN_EXPIRED -> compose.onNodeWithTag("owner-retry").assertDoesNotExist()
        OwnerVisualState.UNCERTAIN_CHECK_ERROR -> compose.onNodeWithTag("owner-retry").assertIsNotEnabled()
        OwnerVisualState.SENT -> compose.onNodeWithTag("owner-done").assertIsEnabled()
        else -> compose.onNodeWithTag("owner-cancel").assertIsEnabled()
    }
}
