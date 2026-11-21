package com.dyapp

import android.content.Context
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class ChatForwardGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishTargetsAndActionsFitAtBothTextScales() = verify("en")

    @Test
    fun russianTargetsAndActionsFitAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val f = ChatForwardUiFixture(compose)
        val context = f.show(language)
        compose.onNodeWithText("Own text").performTouchInput { longClick() }
        compose.onNodeWithTag("msg-forward").performClick()
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { f.scale.floatValue = scale }
            f.ready()
            val title = compose.onNodeWithText(context.getString(R.string.chat_forward_title)).assertIsDisplayed()
            assertTrue("Complete picker title must fit", title.textLayout().textFitsLayout())
            readable(compose.onNodeWithTag("forward-selected-name"))
            readable(compose.onNodeWithTag("forward-selected-peer"))
            val tag = "forward-target-" + f.choices.first()
            compose.awaitNode(hasTestTag(tag)).performScrollTo()
            val id = context.getString(R.string.search_peer, f.choices.first())
            readable(compose.onNode(hasText(id) and hasAnyAncestor(hasTestTag(tag)), useUnmergedTree = true))
            action("chat-forward-confirm", context, R.string.chat_forward)
            action("chat-forward-cancel", context, R.string.remove_cancel)
        }
        compose.onNodeWithTag("chat-forward-cancel").performClick()
        assertEquals(0, f.sends.get())
    }

    private fun readable(node: SemanticsNodeInteraction) {
        node.performScrollTo().assertIsDisplayed()
        assertTrue("Full picker text must fit its measured layout", node.textLayout().textFitsLayout())
        val text = node.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag("chat-forward-dialog").getUnclippedBoundsInRoot()
        assertTrue("Picker text stays inside dialog width", text.left >= dialog.left && text.right <= dialog.right)
    }

    private fun action(tag: String, context: Context, resource: Int) {
        val button = compose.onNodeWithTag(tag).assertIsDisplayed().getUnclippedBoundsInRoot()
        val label = compose.onNode(
            hasText(context.getString(resource)) and hasAnyAncestor(hasTestTag(tag)),
            useUnmergedTree = true
        )
        assertTrue("Full action label must fit", label.textLayout().textFitsLayout())
        val text = label.getUnclippedBoundsInRoot()
        assertTrue("Action label stays inside button width", text.left >= button.left && text.right <= button.right)
        assertTrue("Action label stays inside button height", text.top >= button.top && text.bottom <= button.bottom)
    }
}
