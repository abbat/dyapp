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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntSize
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
    var size by remember { mutableStateOf(IntSize.Zero) }
    var revealed by remember(failure) { mutableStateOf(false) }
    Column(
        Modifier.bringIntoViewRequester(reveal).onSizeChanged { size = it }
            .semantics { this[LinkReviewRecoveryReady] = revealed },
        Arrangement.spacedBy(12.dp),
        content = content
    )
    LaunchedEffect(failure, focused, size) {
        revealed = false
        if (failure != null && focused && size != IntSize.Zero) {
            // A tall footer can already cover the viewport while its feedback is above it.
            reveal.bringIntoView(Rect(Offset.Zero, Size(size.width.toFloat(), 0f)))
            revealed = true
        }
    }
}
