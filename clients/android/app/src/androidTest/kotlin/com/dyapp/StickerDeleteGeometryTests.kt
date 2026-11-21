package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class StickerDeleteGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishConfirmationKeepsFullCopyAndVisibleActionsAtBothTextScales() = verify("en")

    @Test
    fun russianConfirmationKeepsFullCopyAndVisibleActionsAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = StickerDeleteUiFixture(compose)
        val context = fixture.show(language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            fixture.ask()
            requireCopy(context.getString(R.string.chat_sticker_fallback, fixture.own.emoji, fixture.own.packName))
            requireCopy(context.getString(R.string.delete_text))
            requireAction("sticker-delete-confirm", context.getString(R.string.chat_delete))
            requireAction("sticker-delete-cancel", context.getString(R.string.remove_cancel))
            compose.onNodeWithTag("sticker-delete-cancel").performClick()
            fixture.closed()
            assertEquals(0, fixture.deletes.get())
            compose.onNodeWithTag("chat-input").assertTextEquals("keep the draft")
        }
    }

    private fun requireCopy(text: String) {
        val target = hasText(text) and hasAnyAncestor(hasTestTag("sticker-delete-dialog"))
        val node = compose.onNode(target, useUnmergedTree = true).performScrollTo().assertIsDisplayed()
        assertTrue("The complete confirmation copy must fit its layout", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag("sticker-delete-dialog").getUnclippedBoundsInRoot()
        assertTrue(
            "Copy stays inside the current dialog width",
            bounds.left >= dialog.left && bounds.right <= dialog.right
        )
    }

    private fun requireAction(tag: String, text: String) {
        val button = compose.onNodeWithTag(tag).assertIsDisplayed()
        val label = compose.onNode(hasText(text) and hasAnyAncestor(hasTestTag(tag)), useUnmergedTree = true)
        assertTrue("The entire action label must fit", label.textLayout().textFitsLayout())
        val bounds = button.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag("sticker-delete-dialog").getUnclippedBoundsInRoot()
        assertTrue("Action stays inside dialog width", bounds.left >= dialog.left && bounds.right <= dialog.right)
        assertTrue("Action stays inside dialog height", bounds.top >= dialog.top && bounds.bottom <= dialog.bottom)
    }
}
