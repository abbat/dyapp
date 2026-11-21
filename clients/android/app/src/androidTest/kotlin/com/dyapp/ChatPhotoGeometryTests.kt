package com.dyapp

import android.content.Context
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CompletableFuture

class ChatPhotoGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishPhotoStatesFitNormalAndLargeText() = verify("en")

    @Test
    fun russianPhotoStatesFitNormalAndLargeText() = verify("ru")

    private fun verify(language: String) {
        val fixture = ChatPhotoGeometryFixture(compose)
        val context = fixture.show(language)
        for ((index, scale) in listOf(1f, 2f).withIndex()) {
            fixture.response.set(CompletableFuture())
            compose.runOnIdle { fixture.scale.value = scale }
            try {
                verifyStates(fixture, context, index * 2)
            } finally {
                fixture.response.get().complete(fixture.image())
            }
        }
    }

    private fun verifyStates(fixture: ChatPhotoGeometryFixture, context: Context, before: Int) {
        val show = context.getString(R.string.chat_photo_show)
        requireText(show)
        requireText(context.getString(R.string.chat_photo_metadata, fixture.photo.mime, fixture.photo.size.toString()))
        compose.runOnIdle { assertEquals(before, fixture.calls.get()) }
        compose.onNode(hasText(show)).performScrollTo().performClick()
        val download = context.getString(R.string.chat_photo_download)
        requireText(download)
        compose.runOnIdle { assertEquals(before, fixture.calls.get()) }
        compose.onNode(hasText(download)).performScrollTo().performClick()
        requireLoading(context)
        compose.waitUntil(CHECK_WAIT_MS) { fixture.calls.get() == before + 1 }
        fixture.response.get().completeExceptionally(IllegalStateException("controlled refusal"))
        requireText(context.getString(R.string.chat_photo_failed))
        compose.awaitNode(hasText(download) and isEnabled())
        fixture.response.set(CompletableFuture())
        compose.onNode(hasText(download)).performScrollTo().performClick()
        requireLoading(context)
        compose.waitUntil(CHECK_WAIT_MS) { fixture.calls.get() == before + 2 }
        fixture.response.get().complete(fixture.image())
        requireLoaded(context)
        compose.runOnIdle { assertEquals(before + 2, fixture.calls.get()) }
    }

    private fun requireLoaded(context: Context) {
        val image = compose.awaitNode(hasContentDescription(context.getString(R.string.chat_photo)))
            .performScrollTo()
            .assertIsDisplayed()
        requireBounds(image)
        val bounds = image.getUnclippedBoundsInRoot()
        val width = bounds.right.value - bounds.left.value
        val height = bounds.bottom.value - bounds.top.value
        assertEquals("Decoded image keeps its actual aspect", 2f, width / height, 0.01f)
        compose.onNode(hasText(context.getString(R.string.chat_photo_download))).assertDoesNotExist()
        compose.onNode(hasText(context.getString(R.string.chat_photo_failed))).assertDoesNotExist()
    }

    private fun requireLoading(context: Context) {
        val loading = context.getString(R.string.view_loading)
        compose.awaitNode(hasText(loading) and !isEnabled())
        requireText(loading)
    }

    private fun requireText(text: String) {
        val node = compose.awaitNode(hasText(text)).performScrollTo().assertIsDisplayed()
        val label = compose.onNode(
            hasText(text) and SemanticsMatcher.keyIsDefined(SemanticsProperties.Text),
            useUnmergedTree = true
        )
        assertTrue("Complete label must fit", label.textLayout().textFitsLayout())
        requireBounds(node)
    }

    private fun requireBounds(node: SemanticsNodeInteraction) {
        val bounds = node.getUnclippedBoundsInRoot()
        val host = compose.onNodeWithTag("photo-geometry").getUnclippedBoundsInRoot()
        assertTrue("Content stays inside current width", bounds.left >= host.left && bounds.right <= host.right)
        assertTrue("Content scrolls fully into view", bounds.top >= host.top && bounds.bottom <= host.bottom)
    }
}
