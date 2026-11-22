package com.dyapp

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.unit.Density
import org.junit.Assert.assertTrue
import kotlin.math.ceil
import kotlin.math.floor

/** Actual paragraph layouts and live scroll viewports, without guessed padding or tap offsets. */
internal class NativeVisualGeometry(private val compose: ComposeContentTestRule, private val rootTag: String) {

    fun density(expectedScale: Float): Density {
        val nodes = compose.onAllNodes(
            SemanticsMatcher.keyIsDefined(SemanticsActions.GetTextLayoutResult) and
                hasAnyAncestor(hasTestTag(rootTag)),
            useUnmergedTree = true
        ).fetchSemanticsNodes()
        check(nodes.isNotEmpty()) { "Native scale witness has no text layouts" }
        val actual = nodes.map { node(it.id).textLayout().layoutInput.density }
        check(actual.all { it.fontScale == expectedScale }) {
            "Native text scale differs from requested $expectedScale: ${actual.map { it.fontScale }.toSet()}"
        }
        return Density(actual.first().density, actual.first().fontScale)
    }

    fun check(): Int {
        val matcher = SemanticsMatcher.keyIsDefined(SemanticsActions.GetTextLayoutResult) and
            hasAnyAncestor(hasTestTag(rootTag))
        val nodes = compose.onAllNodes(matcher, useUnmergedTree = true).fetchSemanticsNodes()
        check(nodes.isNotEmpty()) { "Dialog exposes no actual text layouts" }
        for (textNode in nodes) {
            val target = node(textNode.id)
            val layout = target.textLayout()
            assertTrue("Complete native text must fit its actual layout", layout.textFitsLayout())
            if (SemanticsProperties.EditableText !in textNode.config) {
                val dialog = compose.onNodeWithTag(rootTag).fetchSemanticsNode()
                val parent = scrollParent(textNode)
                compose.runOnIdle {
                    val left = textNode.positionInRoot.x
                    val viewport = parent?.boundsInRoot?.intersect(dialog.boundsInRoot) ?: dialog.boundsInRoot
                    assertTrue("Text stays within the real viewport", left >= viewport.left)
                    assertTrue("Text stays within the real viewport", left + textNode.size.width <= viewport.right)
                }
                for (line in 0 until layout.lineCount) {
                    exposeLine(textNode, parent, dialog, layout.getLineTop(line)..layout.getLineBottom(line))
                }
            }
        }
        return nodes.size
    }

    private fun exposeLine(
        target: SemanticsNode,
        parent: SemanticsNode?,
        dialog: SemanticsNode,
        line: ClosedFloatingPointRange<Float>
    ) {
        val current = lineGeometry(target, parent, dialog, line)
        if (current.visible) return
        checkNotNull(parent) { "An unscrollable text line is outside the dialog" }
        val distance = current.top - current.viewport.top
        val delta = if (distance < 0f) floor(distance) else ceil(distance)
        node(parent.id).performSemanticsAction(SemanticsActions.ScrollBy) { scroll ->
            check(scroll(0f, delta)) { "Native scroll refused a hidden line" }
        }
        compose.waitUntil(10_000) { lineGeometry(target, parent, dialog, line).visible }
    }

    private fun lineGeometry(
        target: SemanticsNode,
        parent: SemanticsNode?,
        dialog: SemanticsNode,
        line: ClosedFloatingPointRange<Float>
    ): OwnerVisualLine = compose.runOnIdle {
        val textTop = target.positionInRoot.y
        val viewport = parent?.boundsInRoot?.intersect(dialog.boundsInRoot) ?: dialog.boundsInRoot
        OwnerVisualLine(textTop + line.start, textTop + line.endInclusive, viewport)
    }

    private fun scrollParent(node: SemanticsNode): SemanticsNode? =
        generateSequence(node.parent) { it.parent }.firstOrNull { SemanticsActions.ScrollBy in it.config }

    private fun node(id: Int): SemanticsNodeInteraction =
        compose.onNode(SemanticsMatcher("current native node$id") { it.id == id }, useUnmergedTree = true)

    fun first() {
        val body = compose.onAllNodes(
            SemanticsMatcher.keyIsDefined(SemanticsActions.GetTextLayoutResult) and
                hasAnyAncestor(hasTestTag(rootTag)),
            useUnmergedTree = true
        ).fetchSemanticsNodes().firstOrNull { scrollParent(it) != null }
        body?.let { node(it.id).performScrollTo().assertIsDisplayed() }
    }
}

private data class OwnerVisualLine(val top: Float, val bottom: Float, val viewport: Rect) {
    val visible: Boolean get() = top >= viewport.top && bottom <= viewport.bottom
}
