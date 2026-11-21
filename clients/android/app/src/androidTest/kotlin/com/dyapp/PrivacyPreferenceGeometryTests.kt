package com.dyapp

import android.content.Context
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
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

class PrivacyPreferenceGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishSwitchesAndReadOnlyRecoveryKeepFullCopyAtBothTextScales() = verify("en")

    @Test
    fun russianSwitchesAndReadOnlyRecoveryKeepFullCopyAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = PrivacyPreferenceGeometryFixture(compose)
        val context = fixture.show(language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            PrivacyPreferenceKind.entries.forEach { requireLabel(it, context) }
            fixture.failReads.set(true)
            compose.runOnIdle { fixture.owner.value = Any() }
            PrivacyPreferenceKind.entries.forEach { requireFailure(it, context) }
            fixture.failReads.set(false)
            PrivacyPreferenceKind.entries.forEach { kind ->
                compose.onNodeWithTag(kind.statusTag + "-retry").performScrollTo().performClick()
                compose.awaitNode(hasTestTag(kind.tag) and isEnabled())
                compose.onNodeWithTag(kind.statusTag + "-error").assertDoesNotExist()
            }
            assertEquals(0, fixture.writes.get())
        }
    }

    private fun requireLabel(kind: PrivacyPreferenceKind, context: Context) {
        val row = compose.awaitNode(hasTestTag(kind.tag) and isEnabled())
        assertEquals(Role.Switch, row.fetchSemanticsNode().config[SemanticsProperties.Role])
        val matcher = hasText(context.getString(kind.title)) and hasAnyAncestor(hasTestTag(kind.tag))
        val label = compose.onNode(matcher, useUnmergedTree = true)
        readable(label)
        val bounds = label.getUnclippedBoundsInRoot()
        val container = row.getUnclippedBoundsInRoot()
        assertTrue("Label stays inside row width", bounds.left >= container.left && bounds.right <= container.right)
        assertTrue("Label stays inside row height", bounds.top >= container.top && bounds.bottom <= container.bottom)
        kind.hint?.let { readable(compose.onNode(hasText(context.getString(it)), useUnmergedTree = true)) }
    }

    private fun requireFailure(kind: PrivacyPreferenceKind, context: Context) {
        val retryTag = kind.statusTag + "-retry"
        compose.awaitNode(hasTestTag(retryTag) and isEnabled())
        compose.onNodeWithTag(kind.tag).assertIsNotEnabled()
        val error = compose.onNodeWithTag(kind.statusTag + "-error").assertTextEquals(context.getString(kind.failure))
        readable(error)
        val matcher = hasText(context.getString(R.string.privacy_preference_retry)) and
            hasAnyAncestor(hasTestTag(retryTag))
        val label = compose.onNode(matcher, useUnmergedTree = true)
        readable(label)
        val bounds = label.getUnclippedBoundsInRoot()
        val button = compose.onNodeWithTag(retryTag).getUnclippedBoundsInRoot()
        assertTrue("Retry label stays inside button width", bounds.left >= button.left && bounds.right <= button.right)
        assertTrue("Retry label stays inside button height", bounds.top >= button.top && bounds.bottom <= button.bottom)
    }

    private fun readable(node: SemanticsNodeInteraction) {
        node.performScrollTo().assertIsDisplayed()
        assertTrue("The complete privacy copy must fit its text layout", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("privacy-geometry").getUnclippedBoundsInRoot()
        assertTrue("Copy stays inside viewport width", bounds.left >= viewport.left && bounds.right <= viewport.right)
    }
}
