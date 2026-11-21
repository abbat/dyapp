package com.dyapp

import android.content.Context
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

class SavedStickerGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishCollectionAndFullIdConfirmationKeepTheirCopyAtBothTextScales() = verify("en")

    @Test
    fun russianCollectionAndFullIdConfirmationKeepTheirCopyAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = SavedStickerViewFixture()
        val context = fixture.show(compose, language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            requireCopy(fixture.name, "saved-sticker-manager")
            requireCopy(context.getString(R.string.sticker_manager_hint), "saved-sticker-manager")
            requireCopy(context.getString(R.string.sticker_manager_unreadable), "saved-sticker-manager")
            requireAction("saved-sticker-close", R.string.sticker_manager_close, context, "saved-sticker-manager")
            requireConfirmation(fixture, context)
            assertEquals(0, fixture.removals.get())
        }
    }

    private fun requireConfirmation(fixture: SavedStickerViewFixture, context: Context) {
        val id = stickerPackIdText(fixture.second.packId)
        compose.onNodeWithTag("saved-sticker-remove-" + id).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("saved-sticker-remove-id")).assertTextEquals(id)
        requireCopy(context.getString(R.string.sticker_manager_remove_body), "saved-sticker-remove-dialog")
        requireCopy(id, "saved-sticker-remove-dialog")
        requireAction("saved-sticker-confirm", R.string.sticker_manager_confirm, context, "saved-sticker-remove-dialog")
        requireAction("saved-sticker-cancel", R.string.search_cancel, context, "saved-sticker-remove-dialog")
        compose.onNodeWithTag("saved-sticker-cancel").performClick()
        compose.onNodeWithTag("saved-sticker-remove-id").assertDoesNotExist()
        compose.onNodeWithTag("saved-sticker-row-" + id).assertExists()
    }

    private fun requireCopy(text: String, dialogTag: String) {
        val target = hasText(text) and hasAnyAncestor(hasTestTag(dialogTag))
        val node = compose.onNode(target, useUnmergedTree = true).performScrollTo().assertIsDisplayed()
        assertTrue("The full collection/PackID copy must fit", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag(dialogTag).getUnclippedBoundsInRoot()
        assertTrue("Copy stays inside current dialog width", bounds.left >= dialog.left && bounds.right <= dialog.right)
    }

    private fun requireAction(tag: String, resource: Int, context: Context, dialogTag: String) {
        val button = compose.onNodeWithTag(tag).assertIsDisplayed()
        val text = hasText(context.getString(resource)) and hasAnyAncestor(hasTestTag(tag))
        assertTrue("The entire action label must fit", compose.onNode(text, true).textLayout().textFitsLayout())
        val bounds = button.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag(dialogTag).getUnclippedBoundsInRoot()
        assertTrue("Action stays inside dialog width", bounds.left >= dialog.left && bounds.right <= dialog.right)
        assertTrue("Action stays inside dialog height", bounds.top >= dialog.top && bounds.bottom <= dialog.bottom)
    }
}
