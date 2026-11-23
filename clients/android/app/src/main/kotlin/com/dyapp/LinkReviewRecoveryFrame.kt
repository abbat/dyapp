package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

internal val LinkReviewRecoveryReady = SemanticsPropertyKey<Boolean>("LinkReviewRecoveryReady")

/** Reveal feedback after safe focus; completion is distinct from receiving focus. */
@Composable
internal fun LinkReviewRecoveryFrame(
    failure: Throwable?,
    focused: Boolean,
    content: @Composable ColumnScope.() -> Unit
) {
    val reveal = remember { BringIntoViewRequester() }
    var leading by remember { mutableStateOf<Rect?>(null) }
    var revealed by remember(failure) { mutableStateOf(false) }
    Column(
        Modifier.bringIntoViewRequester(reveal).onGloballyPositioned { leading = feedbackLeadingRect(it) }
            .semantics { this[LinkReviewRecoveryReady] = revealed },
        Arrangement.spacedBy(12.dp),
        content = content
    )
    LaunchedEffect(failure, focused, leading) {
        revealed = false
        if (failure != null && focused) {
            // A non-empty measured leading region remains meaningful for an oversized footer.
            leading?.let {
                reveal.bringIntoView(it)
                revealed = true
            }
        }
    }
}

private fun feedbackLeadingRect(coordinates: LayoutCoordinates): Rect? {
    if (!coordinates.isAttached || coordinates.size.width <= 0) return null
    val baseline = coordinates[FirstBaseline]
    return if (baseline != AlignmentLine.Unspecified && baseline > 0) {
        Rect(0f, 0f, coordinates.size.width.toFloat(), baseline.toFloat())
    } else {
        null
    }
}
