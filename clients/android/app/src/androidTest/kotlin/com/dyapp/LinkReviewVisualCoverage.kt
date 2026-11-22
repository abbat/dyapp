package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag

/** Coverage names actual laid-out value lines, including paragraphs taller than one viewport. */
internal class LinkReviewVisualCoverage(private val compose: ComposeContentTestRule) {
    private val valueTag = Regex("link-review-[A-Z_]+-(LOCAL|REMOTE)-value")

    fun required(): Map<String, List<Int>> = values().associate { node ->
        tag(node) to layout(node).let { (0 until it.lineCount).toList() }
    }

    fun visible(): Map<String, List<Int>> {
        val viewport = compose.onNodeWithTag("settings-receive").fetchSemanticsNode().boundsInRoot
        return values().associate { node ->
            val layout = layout(node)
            val top = node.positionInRoot.y
            tag(node) to (0 until layout.lineCount).filter { line ->
                top + layout.getLineTop(line) >= viewport.top &&
                    top + layout.getLineBottom(line) <= viewport.bottom
            }
        }
    }

    private fun tag(node: SemanticsNode): String = checkNotNull(node.config.getOrNull(SemanticsProperties.TestTag))

    private fun layout(node: SemanticsNode) =
        compose.onNode(SemanticsMatcher("comparison value ${node.id}") { it.id == node.id }, useUnmergedTree = true)
            .textLayout()

    private fun values(): List<SemanticsNode> = compose.onAllNodes(
        SemanticsMatcher.keyIsDefined(SemanticsActions.GetTextLayoutResult) and
            hasAnyAncestor(hasTestTag("settings-receive")) and
            SemanticsMatcher("actual comparison value") {
                valueTag.matches(it.config.getOrNull(SemanticsProperties.TestTag).orEmpty())
            },
        useUnmergedTree = true
    ).fetchSemanticsNodes().also { check(it.isNotEmpty()) { "Comparison exposes no value layouts" } }
}
