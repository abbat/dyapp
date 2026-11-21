package com.dyapp

import android.content.Context
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
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
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveFailure
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus

class StickerPackGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishPackStatusWarningsAndControlsFitAtBothTextScales() = verify("en")

    @Test
    fun russianPackStatusWarningsAndControlsFitAtBothTextScales() = verify("ru")

    private fun status(state: StickerPackSaveState, failure: StickerPackSaveFailure? = null) =
        StickerPackSaveStatus(state, ByteArray(16), failure, null, listOf(StickerPackSaveControl.MANAGE))

    private fun variants(): List<StickerPackSaveStatus> =
        listOf(
            StickerPackSaveState.NEW,
            StickerPackSaveState.SAVED,
            StickerPackSaveState.WAITING,
            StickerPackSaveState.DOWNLOADING
        ).map { status(it) } +
            StickerPackSaveFailure.entries.map { status(StickerPackSaveState.FAILED, it) } +
            status(StickerPackSaveState.FAILED)

    private fun verify(language: String) {
        val fixture = StickerPackGeometryFixture(compose)
        val context = fixture.show(language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            fixture.holdRead().use {
                requireCopy(context)
                requireAction(StickerPackSaveControl.REFRESH)
                requireAction(StickerPackSaveControl.MANAGE)
            }
            for (snapshot in variants()) {
                fixture.present(snapshot)
                requireCopy(context)
                requireAction(StickerPackSaveControl.MANAGE)
            }
            // Exercise every projected label, not a duplicate eligibility table.
            fixture.present(status(StickerPackSaveState.WAITING).copy(controls = StickerPackSaveControl.entries))
            StickerPackSaveControl.entries.forEach(::requireAction)
            requireReadFailure(fixture, context)
            assertEquals(0, fixture.writes.get())
        }
    }

    private fun requireReadFailure(fixture: StickerPackGeometryFixture, context: Context) {
        fixture.failReads.set(true)
        compose.onNodeWithTag("sticker-pack-REFRESH").performScrollTo().performClick()
        val failure = hasText(context.getString(R.string.sticker_pack_read_failed))
        val message = compose.awaitNode(failure).performScrollTo().assertIsDisplayed()
        assertTrue(message.textLayout().textFitsLayout())
        requireCopy(context)
        requireAction(StickerPackSaveControl.REFRESH)
        requireAction(StickerPackSaveControl.MANAGE)
        compose.onNodeWithTag("sticker-pack-REQUEST").assertDoesNotExist()
    }

    private fun requireCopy(context: Context) {
        val caption = compose.onNodeWithTag("sticker-pack-caption").performScrollTo().assertIsDisplayed()
        assertTrue("Complete status text must fit", caption.textLayout().textFitsLayout())
        requireWidth(caption)
        val warning = compose.onNode(hasText(context.getString(R.string.sticker_pack_warning)))
            .performScrollTo()
            .assertIsDisplayed()
        assertTrue("Complete warning must fit", warning.textLayout().textFitsLayout())
        requireWidth(warning)
    }

    private fun requireAction(control: StickerPackSaveControl) {
        val tag = "sticker-pack-" + control.name
        val button = compose.onNodeWithTag(tag).performScrollTo().assertIsDisplayed()
        val label = compose.onNode(
            hasAnyAncestor(hasTestTag(tag)) and SemanticsMatcher.keyIsDefined(SemanticsProperties.Text),
            useUnmergedTree = true
        )
        assertTrue("Complete action label must fit", label.textLayout().textFitsLayout())
        requireWidth(button)
        val bounds = button.getUnclippedBoundsInRoot()
        val host = compose.onNodeWithTag("pack-geometry").getUnclippedBoundsInRoot()
        assertTrue("Action must scroll fully into view", bounds.top >= host.top && bounds.bottom <= host.bottom)
    }

    private fun requireWidth(node: SemanticsNodeInteraction) {
        val bounds = node.getUnclippedBoundsInRoot()
        val host = compose.onNodeWithTag("pack-geometry").getUnclippedBoundsInRoot()
        assertTrue("Content must stay inside current width", bounds.left >= host.left && bounds.right <= host.right)
    }
}
