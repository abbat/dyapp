package com.dyapp

import android.content.Context
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class ChatClearGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishWarningKeepsTargetAndConsequencesAtBothTextScales() = verify("en")

    @Test
    fun russianWarningKeepsTargetAndConsequencesAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = ChatClearFixture(compose)
        val context = fixture.show(language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.floatValue = scale }
            fixture.ready()
            requireOrder(context)
            readable(compose.onNodeWithTag("chat-clear-name"))
            readable(compose.onNodeWithTag("chat-clear-peer"))
            for (resource in listOf(R.string.clear_text, R.string.clear_pending, R.string.clear_later)) {
                readable(compose.onNode(hasText(context.getString(resource))))
            }
            compose.onNodeWithTag("chat-clear-confirm").assertIsDisplayed()
            compose.onNodeWithTag("chat-clear-cancel").assertIsDisplayed()
        }
        compose.onNodeWithTag("chat-clear-cancel").performClick()
        compose.onNodeWithTag("chat-clear-dialog").assertDoesNotExist()
    }

    private fun requireOrder(context: Context) {
        val nodes = listOf(
            compose.onNodeWithTag("chat-clear-name"),
            compose.onNodeWithTag("chat-clear-peer"),
            compose.onNodeWithTag("copy-chat-clear-peer")
        ) + listOf(R.string.clear_text, R.string.clear_pending, R.string.clear_later).map {
            compose.onNode(hasText(context.getString(it)))
        }
        val bounds = nodes.map { it.getUnclippedBoundsInRoot() }
        bounds.zipWithNext().forEach { (before, after) ->
            assertTrue("Target, copy and consequences retain reading order", before.bottom <= after.top)
        }
    }

    private fun readable(node: SemanticsNodeInteraction) {
        node.performScrollTo().assertIsDisplayed()
        assertTrue("Complete warning text must fit its measured layout", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag("chat-clear-dialog").getUnclippedBoundsInRoot()
        assertTrue("Warning text stays inside dialog width", bounds.left >= dialog.left && bounds.right <= dialog.right)
    }
}
