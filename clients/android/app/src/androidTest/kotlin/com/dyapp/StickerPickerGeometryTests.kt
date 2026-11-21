package com.dyapp

import android.content.Context
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class StickerPickerGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishWideAndTallChoicesKeepAspectAndFullLabelsAtBothTextScales() = verify("en")

    @Test
    fun russianWideAndTallChoicesKeepAspectAndFullLabelsAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = StickerPickerUiFixture(compose)
        val context = fixture.show(language)
        fixture.recent()
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            assertChoice(fixture, fixture.wide, context)
            compose.onNodeWithTag("sticker-picker-next").performScrollTo().performClick()
            compose.awaitNode(hasTestTag(fixture.choiceTag(fixture.tall)) and isEnabled())
            assertChoice(fixture, fixture.tall, context)
            compose.onNodeWithTag("sticker-picker-back").performScrollTo().performClick()
            compose.awaitNode(hasTestTag(fixture.choiceTag(fixture.wide)) and isEnabled())
            compose.onNodeWithTag("sticker-picker-cancel").assertIsDisplayed()
            compose.onNodeWithTag("sticker-picker-manage").assertIsDisplayed()
        }
    }

    private fun assertChoice(fixture: StickerPickerUiFixture, choice: StickerPickerChoice, context: Context) {
        val tag = fixture.choiceTag(choice)
        val label = context.getString(R.string.chat_sticker_fallback, choice.sticker.emoji, choice.name)
        compose.onNodeWithTag(tag).assertContentDescriptionEquals(label)
        val imageTag = tag.replace("sticker-picker-choice-", "sticker-picker-image-")
        val image = compose.onNodeWithTag(imageTag, useUnmergedTree = true).performScrollTo().assertIsDisplayed()
        val bounds = image.getUnclippedBoundsInRoot()
        val ratio = (bounds.right - bounds.left).value / (bounds.bottom - bounds.top).value
        assertEquals(choice.sticker.width.toFloat() / choice.sticker.height.toFloat(), ratio, 0.01f)
        val text = compose.onNodeWithText(label, useUnmergedTree = true).performScrollTo().assertIsDisplayed()
        assertTrue("The complete pack label must fit", text.textLayout().textFitsLayout())
        val dialog = compose.onNodeWithTag("sticker-picker").getUnclippedBoundsInRoot()
        val textBounds = text.getUnclippedBoundsInRoot()
        assertTrue(
            "Label stays inside the current dialog",
            textBounds.left >= dialog.left && textBounds.right <= dialog.right
        )
    }
}
