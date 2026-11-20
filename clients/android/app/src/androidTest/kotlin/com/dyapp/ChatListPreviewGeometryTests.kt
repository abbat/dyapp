package com.dyapp

import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.width
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody

private const val BOUNDED_PREVIEW = "One two three four five six seven eight nine ten eleven twelve thirteen."

class ChatListPreviewGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    private fun node(tag: String) = compose.onNodeWithTag(tag, useUnmergedTree = true)

    @Composable
    private fun Content(scale: Float) {
        val base = LocalDensity.current
        CompositionLocalProvider(
            LocalDensity provides Density(base.density, scale),
            LocalActiveChatPeers provides setOf("conversation")
        ) {
            val natural = rememberTextMeasurer().measure(BOUNDED_PREVIEW, MaterialTheme.typography.bodyMedium)
            val naturalWidth = with(LocalDensity.current) { natural.size.width.toDp() }
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val width = minOf(maxWidth * (2f / 3f), naturalWidth / 2)
                Column(Modifier.width(width).verticalScroll(rememberScrollState())) {
                    ChatListRow(
                        previewRow(ChatListPreview(ChatPreviewBody.Text(BOUNDED_PREVIEW), false, 86_400_000uL)),
                        null,
                        Modifier.testTag("geometry-row")
                    )
                }
            }
        }
    }

    @Test
    fun boundedTextWrapsCompletelyAndMetadataCannotStealItsWidthAtLargeFontScale() {
        val scale = mutableStateOf(1f)
        compose.setContent { MaterialTheme { Content(scale.value) } }
        for (value in listOf(1f, 2f)) {
            compose.runOnIdle { scale.value = value }
            node("chat-preview").assertTextEquals(BOUNDED_PREVIEW)
            val layout = node("chat-preview").textLayout()
            assertTrue("The measured fixture must wrap at scale $value", layout.lineCount > 1)
            assertTrue("All preview characters must fit at scale $value", layout.textFitsLayout())
            assertGeometry()
            node("chat-preview-time").performScrollTo().assertIsDisplayed()
            val timeLayout = node("chat-preview-time").textLayout()
            assertTrue("The current localized timestamp must fit", timeLayout.textFitsLayout())
            node("chat-muted").assertExists()
            node("chat-presence-conversation").assertExists()
        }
    }

    private fun assertGeometry() {
        val row = node("geometry-row").getUnclippedBoundsInRoot()
        val preview = node("chat-preview").getUnclippedBoundsInRoot()
        val time = node("chat-preview-time").getUnclippedBoundsInRoot()
        assertTrue("Preview has actual layout width", preview.width.value > 0f)
        assertTrue("Preview stays inside row", preview.left >= row.left && preview.right <= row.right)
        assertTrue("Metadata follows preview without overlap", time.top >= preview.bottom)
        assertTrue("Timestamp stays inside row", time.left >= row.left && time.right <= row.right)
    }
}
