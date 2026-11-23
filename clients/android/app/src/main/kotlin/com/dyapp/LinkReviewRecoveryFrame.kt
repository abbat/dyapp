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
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp

internal val LinkReviewRecoveryReady = SemanticsPropertyKey<Boolean>("LinkReviewRecoveryReady")

/** Reveal measured feedback after safe focus, independently of an oversized action footer. */
@Composable
internal fun LinkReviewRecoveryFrame(
    failure: Throwable?,
    focused: Boolean,
    feedback: @Composable ColumnScope.() -> Unit,
    content: @Composable ColumnScope.() -> Unit
) {
    val reveal = remember { BringIntoViewRequester() }
    var measured by remember { mutableStateOf(IntSize.Zero) }
    var revealed by remember(failure) { mutableStateOf(false) }
    Column(
        Modifier.semantics { this[LinkReviewRecoveryReady] = revealed },
        Arrangement.spacedBy(12.dp)
    ) {
        if (failure != null) {
            Column(
                Modifier.bringIntoViewRequester(reveal).onSizeChanged { measured = it },
                Arrangement.spacedBy(12.dp),
                content = feedback
            )
        }
        content()
    }
    LaunchedEffect(failure, focused, measured) {
        revealed = false
        if (failure == null || !focused) return@LaunchedEffect
        if (measured.width > 0 && measured.height > 0) {
            reveal.bringIntoView()
            revealed = true
        }
    }
}
