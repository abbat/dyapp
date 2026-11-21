package com.dyapp

import android.content.Context
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class SavedStickerStateGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishPendingAndFailedManagerStatesFitBothTextScales() = verify("en")

    @Test
    fun russianPendingAndFailedManagerStatesFitBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = SavedStickerViewFixture()
        val context = fixture.show(compose, language)
        for ((index, scale) in listOf(1f, 2f).withIndex()) {
            compose.runOnIdle { fixture.scale.value = scale }
            verifyReads(fixture, context)
            verifyImports(fixture, context, index * 3)
            assertEquals(0, fixture.removals.get())
        }
    }

    private fun verifyReads(fixture: SavedStickerViewFixture, context: Context) {
        val gate = CompletableDeferred<Unit>()
        fixture.readGate.set(gate)
        try {
            action("refresh", true)
            copy(context, R.string.sticker_manager_loading)
            action("import", false)
        } finally { gate.complete(Unit) }
        awaitEditable()
        fixture.readFailure.set(true)
        action("refresh", true)
        copy(context, R.string.sticker_manager_read_failed)
        action("import", false)
        compose.onNodeWithTag("saved-sticker-row-" + stickerPackIdText(fixture.second.packId)).assertExists()
        fixture.readFailure.set(false)
        action("refresh", true)
        awaitEditable()
    }

    private fun verifyImports(fixture: SavedStickerViewFixture, context: Context, before: Int) {
        fixture.importFailure.set(false)
        fixture.added.set(true)
        val gate = CompletableDeferred<Unit>()
        fixture.importGate.set(gate)
        try {
            action("import", true)
            copy(context, R.string.sticker_manager_loading)
            action("refresh", false)
            compose.waitUntil(CHECK_WAIT_MS) { fixture.imports.get() == before + 1 }
        } finally { gate.complete(Unit) }
        copy(context, R.string.sticker_manager_added)
        awaitEditable()
        fixture.added.set(false)
        action("import", true)
        copy(context, R.string.sticker_manager_already)
        awaitEditable()
        fixture.importFailure.set(true)
        action("import", true)
        copy(context, R.string.sticker_manager_action_failed)
        awaitEditable()
        assertEquals(before + 3, fixture.imports.get())
    }

    private fun awaitEditable() {
        compose.awaitNode(hasTestTag("saved-sticker-import") and isEnabled())
    }

    private fun action(name: String, enabled: Boolean) {
        val tag = "saved-sticker-$name"
        val status = if (enabled) isEnabled() else !isEnabled()
        val node = compose.awaitNode(hasTestTag(tag) and status).performScrollTo().assertIsDisplayed()
        val label = compose.onNode(
            hasAnyAncestor(hasTestTag(tag)) and SemanticsMatcher.keyIsDefined(SemanticsProperties.Text),
            useUnmergedTree = true
        )
        assertTrue("Complete action label fits", label.textLayout().textFitsLayout())
        requireBounds(node.getUnclippedBoundsInRoot())
        if (enabled) node.performClick()
    }

    private fun copy(context: Context, resource: Int) {
        val node = compose.awaitNode(hasText(context.getString(resource))).performScrollTo().assertIsDisplayed()
        assertTrue("Complete state text fits", node.textLayout().textFitsLayout())
        requireBounds(node.getUnclippedBoundsInRoot())
    }

    private fun requireBounds(bounds: androidx.compose.ui.unit.DpRect) {
        val host = compose.onNodeWithTag("saved-sticker-manager").getUnclippedBoundsInRoot()
        assertTrue("State stays inside current width", bounds.left >= host.left && bounds.right <= host.right)
        assertTrue("State scrolls fully into view", bounds.top >= host.top && bounds.bottom <= host.bottom)
    }
}
