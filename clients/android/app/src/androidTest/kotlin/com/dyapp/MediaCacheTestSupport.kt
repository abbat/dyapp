package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import uniffi.dyapp.ChatMediaCache
import uniffi.dyapp.ChatMediaCacheSettings

internal fun cacheSettings(limit: String = "256", bytes: ULong = 268_435_456uL) =
    ChatMediaCacheSettings(ChatMediaCache(true, bytes, 8uL), limit, "1", "10240")

@Composable
internal fun MediaCacheTestHost(owner: Any, calls: MediaCacheCalls) {
    MaterialTheme {
        Column(Modifier.verticalScroll(rememberScrollState())) { MediaCacheControls(owner, calls) }
    }
}

internal fun ComposeTestRule.cacheReady(tag: String) = awaitNode(hasTestTag(tag) and isEnabled())

internal fun ComposeTestRule.cacheClick(tag: String) {
    cacheReady(tag).performScrollTo().performClick()
}

internal fun ComposeTestRule.cacheDraft(): String =
    onNodeWithTag("cache-limit").fetchSemanticsNode().config[SemanticsProperties.EditableText].text
