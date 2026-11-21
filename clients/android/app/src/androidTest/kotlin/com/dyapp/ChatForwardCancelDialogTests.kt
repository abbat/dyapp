package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

class ChatForwardCancelDialogTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun realCancelClosesAnUnfinishedReadAndOldCallbacksCannotCloseTheReplacement() {
        val f = ForwardCancelDialogFixture(compose)
        try {
            f.show()
            compose.waitUntil(60_000) { f.entered.isDone }
            val cancel = compose.onNodeWithTag("chat-forward-cancel").assertIsEnabled()
            val old = cancel.fetchSemanticsNode().config[applicationClickKey]
            cancel.performClick()
            compose.onNodeWithTag("chat-forward-dialog").assertDoesNotExist()
            assertFalse(f.response.isDone)
            compose.runOnIdle { f.state.ask(forwardSource()) }
            compose.awaitNode(hasTestTag("chat-forward-confirm") and isEnabled())
            compose.runOnIdle { old() }
            f.release()
            compose.onNodeWithTag("chat-forward-dialog").assertExists()
            compose.runOnIdle {
                assertEquals(listOf("fresh"), f.state.targets)
                assertNull(f.state.failure)
            }
            assertEquals(2, f.reads.get())
            assertEquals(0, f.sends.get())
        } finally { f.release() }
    }
}
