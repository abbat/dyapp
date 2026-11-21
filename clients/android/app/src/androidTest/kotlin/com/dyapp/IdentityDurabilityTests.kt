package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.Locale

class IdentityDurabilityTests {
    @get:Rule
    val compose = createComposeRule()
    private val ready = Opened.Ready(Slot("notice", File("presentation-only"), SlotState.READY), "a".repeat(64))
    private val shown = mutableStateOf(Shown(ready))
    private val scale = mutableFloatStateOf(1f)

    private fun show(language: String) = InstrumentationRegistry.getInstrumentation().targetContext.let { base ->
        val configuration = Configuration(
            base.resources.configuration
        ).apply { setLocale(Locale.forLanguageTag(language)) }
        base.createConfigurationContext(configuration).also { localized ->
            compose.setContent {
                LocalizedTestContext(localized, scale.floatValue) {
                    MaterialTheme {
                        Text(ready.peerId, Modifier.testTag("notice-route"))
                        IdentityDurabilityNotice(shown.value)
                    }
                }
            }
        }
    }

    @Test
    fun englishCommittedNoticeFitsWithoutChangingSelection() = verify("en")

    @Test
    fun russianCommittedNoticeFitsWithoutChangingSelection() = verify("ru")

    private fun verify(language: String) {
        val context = show(language)
        compose.onNodeWithTag("identity-durability-dialog").assertDoesNotExist()
        for (fontScale in listOf(1f, 2f)) {
            val result = Shown(ready, durabilityDetail = "private native diagnostic")
            compose.runOnIdle {
                scale.floatValue = fontScale
                shown.value = result
            }
            val notice = compose.awaitNode(hasTestTag("identity-durability-notice"))
            notice.assertTextEquals(context.getString(R.string.identity_persistence_notice))
            notice.performScrollTo().assertIsDisplayed()
            assertEquals(LiveRegionMode.Polite, notice.fetchSemanticsNode().config[SemanticsProperties.LiveRegion])
            assertTrue("Complete notice fits its measured text layout", notice.textLayout().textFitsLayout())
            val title = compose.onNodeWithTag("identity-durability-title").assertIsDisplayed()
            assertTrue("Complete notice title fits", title.textLayout().textFitsLayout())
            acknowledgement(context).performClick()
            compose.awaitNode(hasTestTag("notice-route")).assertTextEquals(ready.peerId)
            compose.onNodeWithTag("identity-durability-dialog").assertDoesNotExist()
            compose.runOnIdle {
                assertSame(result, shown.value)
                assertSame(ready, shown.value.opened)
            }
        }
    }

    private fun acknowledgement(context: Context): SemanticsNodeInteraction {
        val button = compose.onNodeWithTag("identity-durability-ack").assertIsDisplayed()
        val acknowledgement = context.getString(R.string.identity_persistence_ack)
        val label = compose.onNode(
            hasText(acknowledgement) and hasAnyAncestor(hasTestTag("identity-durability-ack")),
            useUnmergedTree = true
        )
        assertTrue("Complete acknowledgement fits", label.textLayout().textFitsLayout())
        val bounds = button.getUnclippedBoundsInRoot()
        val text = label.getUnclippedBoundsInRoot()
        assertTrue(text.left >= bounds.left && text.right <= bounds.right)
        assertTrue(text.top >= bounds.top && text.bottom <= bounds.bottom)
        return button
    }

    @Test
    fun retiredAcknowledgementCannotDismissAnEqualSuccessorAndSyncedResultClearsNotice() {
        show("en")
        compose.runOnIdle { shown.value = Shown(ready, durabilityDetail = "unconfirmed") }
        val node = compose.awaitNode(hasTestTag("identity-durability-ack")).fetchSemanticsNode()
        val old = checkNotNull(node.config[SemanticsActions.OnClick].action)
        val successor = Shown(ready, durabilityDetail = "unconfirmed")
        compose.runOnIdle { shown.value = successor }
        compose.awaitNode(hasTestTag("identity-durability-ack"))
        compose.runOnIdle {
            old()
            assertSame(successor, shown.value)
        }
        compose.onNodeWithTag("identity-durability-dialog").assertIsDisplayed()
        compose.runOnIdle { shown.value = Shown(ready) }
        compose.onNodeWithTag("identity-durability-dialog").assertDoesNotExist()
        compose.runOnIdle {
            old()
            assertSame(ready, shown.value.opened)
        }
        compose.onNodeWithTag("identity-durability-dialog").assertDoesNotExist()
    }
}
