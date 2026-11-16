package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.text.TextLayoutResult

/** Read the current native text layout through its semantic completion action. */
internal fun SemanticsNodeInteraction.textLayout(): TextLayoutResult {
    val layouts = mutableListOf<TextLayoutResult>()
    performSemanticsAction(SemanticsActions.GetTextLayoutResult) { read -> check(read(layouts)) }
    return layouts.single()
}

/** Paragraph containers can be wider than their content; test actual lines and complete text. */
internal fun TextLayoutResult.textFitsLayout(): Boolean =
    !didOverflowHeight && lineCount > 0 && getLineEnd(lineCount - 1) == layoutInput.text.length &&
        (0 until lineCount).all { line ->
            !isLineEllipsized(line) &&
                getLineLeft(line) >= 0f && getLineRight(line) <= size.width.toFloat() &&
                getLineTop(line) >= 0f && getLineBottom(line) <= size.height.toFloat()
        }
