package com.dyapp

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import com.dyapp.ui.theme.DYAppTheme
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Calibrate real platform layouts before trusting text-fit assertions in screen tests. */
class TextLayoutAssertionsTests {
    @get:Rule
    val compose = createComposeRule()

    private fun show(
        softWrap: Boolean = true,
        maxLines: Int = Int.MAX_VALUE,
        overflow: TextOverflow = TextOverflow.Clip,
        clipHeight: Boolean = false,
    ): TextLayoutResult {
        compose.setContent {
            DYAppTheme {
                val text = "one two three four five six seven eight"
                val style = MaterialTheme.typography.bodyLarge
                val natural = rememberTextMeasurer().measure(text, style)
                val density = LocalDensity.current
                val width = with(density) { (natural.size.width / 2f).toDp() }
                val height = with(density) { (natural.size.height / 2f).toDp() }
                val modifier = Modifier.width(width).testTag("measured-text")
                // A root Text inherits exact window minima, which can override its requested size.
                Box(Modifier.fillMaxSize()) {
                    Text(
                        text,
                        if (clipHeight) modifier.height(height) else modifier,
                        style = style,
                        softWrap = softWrap,
                        maxLines = maxLines,
                        overflow = overflow,
                    )
                }
            }
        }
        return compose.onNodeWithTag("measured-text").assertIsDisplayed().textLayout()
    }

    private fun TextLayoutResult.diagnostic(): String =
        "size=$size constraints=${layoutInput.constraints} lines=$lineCount " +
            "overflow=$didOverflowWidth/$didOverflowHeight lastEnd=${getLineEnd(lineCount - 1)} " +
            "textLength=${layoutInput.text.length} right=${getLineRight(0)} ellipsis=${isLineEllipsized(0)}"

    @Test
    fun wrappingAllCharactersInsideMeasuredLinesPasses() {
        val layout = show()
        assertTrue(layout.diagnostic(), layout.lineCount > 1)
        assertTrue(layout.diagnostic(), layout.textFitsLayout())
    }

    @Test
    fun unwrappedTextBeyondItsMeasuredWidthFails() {
        val layout = show(softWrap = false)
        assertTrue(layout.diagnostic(), layout.getLineRight(0) > layout.size.width)
        assertFalse(layout.textFitsLayout())
    }

    @Test
    fun ellipsizedTextFailsEvenWhenItsVisibleLineFits() {
        val layout = show(maxLines = 1, overflow = TextOverflow.Ellipsis)
        assertTrue(layout.diagnostic(), layout.isLineEllipsized(0))
        assertFalse(layout.textFitsLayout())
    }

    @Test
    fun aClippedLineLimitCannotHideRemainingCharacters() {
        val layout = show(maxLines = 1)
        assertTrue(layout.didOverflowHeight)
        assertFalse(layout.textFitsLayout())
    }

    @Test
    fun textBeyondItsMeasuredHeightFails() {
        val layout = show(clipHeight = true)
        assertTrue(layout.didOverflowHeight)
        assertFalse(layout.textFitsLayout())
    }
}
