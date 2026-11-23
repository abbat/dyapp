package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.AccessibilityAction
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.font.FontFamily
import java.util.concurrent.atomic.AtomicReference

private const val IDENTIFIER_GROUP_SIZE = 8

/** Display groups are presentation only; accessibility and action targets retain the exact raw value. */
@Composable
internal fun IdentifierText(raw: String, modifier: Modifier = Modifier) {
    val measured = remember(raw) { AtomicReference<TextLayoutResult>() }
    Text(
        raw.chunked(IDENTIFIER_GROUP_SIZE).joinToString(" "),
        modifier.clearAndSetSemantics {
            text = AnnotatedString(raw)
            this[SemanticsActions.GetTextLayoutResult] = AccessibilityAction(null) { results ->
                measured.get()?.let { results.add(it) } ?: false
            }
        },
        fontFamily = FontFamily.Monospace,
        onTextLayout = measured::set
    )
}
