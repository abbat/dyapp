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
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp

/** Reveal feedback and recovery together after the safe focus target has actually received focus. */
@Composable
internal fun LinkReviewRecoveryFrame(
    failure: Throwable?,
    focused: Boolean,
    content: @Composable ColumnScope.() -> Unit
) {
    val reveal = remember { BringIntoViewRequester() }
    var size by remember { mutableStateOf(IntSize.Zero) }
    Column(
        Modifier.bringIntoViewRequester(reveal).onSizeChanged { size = it },
        Arrangement.spacedBy(12.dp),
        content = content
    )
    LaunchedEffect(failure, focused, size) {
        if (failure != null && focused && size != IntSize.Zero) reveal.bringIntoView()
    }
}
