package com.dyapp

import android.content.res.Configuration
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.width
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LocalCollectionEntry
import java.io.File
import java.util.Locale

private const val COMPLETE_COLLECTION_PEER = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
private enum class CollectionRowKind { IDENTITY, BLOCKED, PASSED, VIEW }

class CollectionRowGeometryTests {
    @get:Rule
    val compose = createComposeRule()
    private val language = mutableStateOf("en")
    private val dark = mutableStateOf(false)
    private val scale = mutableStateOf(1f)
    private val kind = mutableStateOf(CollectionRowKind.IDENTITY)

    private fun context() = InstrumentationRegistry.getInstrumentation().targetContext.let { base ->
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language.value))
        }
        base.createConfigurationContext(configuration)
    }

    @Composable
    private fun Content() {
        val localized = context()
        LocalizedTestContext(localized, scale.value) {
            DYAppTheme(dark.value) {
                BoxWithConstraints(Modifier.fillMaxSize()) {
                    Column(
                        Modifier.width(maxWidth * (2f / 3f)).testTag("collection-geometry")
                            .verticalScroll(rememberScrollState())
                    ) { ActualRow() }
                }
            }
        }
    }

    @Composable
    private fun ActualRow() = when (kind.value) {
        CollectionRowKind.IDENTITY -> {
            val slot = Slot("geometry-identity", File("build/unopened-geometry"), SlotState.READY)
            val opened = Opened.Ready(slot, COMPLETE_COLLECTION_PEER).apply { removalOffered = true }
            IdentityCollectionRow(opened, slot.id, 1) {}
        }
        CollectionRowKind.BLOCKED -> {
            PeopleCollectionRow(COMPLETE_COLLECTION_PEER, 1) { _, number ->
                BlockedActions(number, false, {}, {})
            }
        }
        CollectionRowKind.VIEW -> {
            ReceivedViewRow(LocalCollectionEntry(COMPLETE_COLLECTION_PEER, "2024-02-29"), 1, true) {}
        }
        CollectionRowKind.PASSED -> {
            PeopleCollectionRow(COMPLETE_COLLECTION_PEER, 1) { _, _ ->
                TextButton({}, Modifier.testTag("unpass-1")) { Text(context().getString(R.string.settings_unpass)) }
            }
        }
    }

    @Test
    fun fullIdentifiersAndLocalizedActionsFitCurrentRowsAtNormalAndLargeText() {
        compose.setContent { Content() }
        for (locale in listOf("en", "ru")) {
            for (fontScale in listOf(1f, 2f)) {
                for (row in CollectionRowKind.entries) {
                    checkCase(locale, fontScale, row, false)
                    checkCase(locale, fontScale, row, true)
                }
            }
        }
    }

    private fun checkCase(locale: String, fontScale: Float, row: CollectionRowKind, darkTheme: Boolean) {
        compose.runOnIdle {
            language.value = locale
            scale.value = fontScale
            kind.value = row
            dark.value = darkTheme
        }
        assertIdentifier()
        if (row == CollectionRowKind.VIEW) assertDate()
        actions(row).forEach { (tag, text) -> assertAction(tag, text) }
    }

    private fun assertIdentifier() {
        val node = compose.onNodeWithText(COMPLETE_COLLECTION_PEER, useUnmergedTree = true)
        node.performScrollTo().assertTextEquals(COMPLETE_COLLECTION_PEER)
        val layout = node.textLayout()
        assertEquals("Actual native text scale", scale.value, layout.layoutInput.density.fontScale, 0f)
        assertTrue("Complete identifier must wrap", layout.lineCount > 1)
        assertTrue("Every identifier character must fit", layout.textFitsLayout())
        assertGroupedIdentifier(layout)
    }

    private fun assertGroupedIdentifier(layout: TextLayoutResult) {
        val displayed = layout.layoutInput.text.text
        assertEquals(
            "Display groups preserve the complete raw target",
            COMPLETE_COLLECTION_PEER,
            displayed.replace(" ", "")
        )
        for (line in 0 until layout.lineCount) {
            val start = layout.getLineStart(line)
            val end = layout.getLineEnd(line, visibleEnd = true)
            val visible = displayed.substring(start, end).trim()
            assertTrue(
                "Line $line breaks only between fixed groups: $visible",
                visible.split(' ').all { it.length == 8 }
            )
        }
    }

    private fun actions(row: CollectionRowKind): List<Pair<String, String>> = when (row) {
        CollectionRowKind.IDENTITY -> listOf("settings-remove" to context().getString(R.string.identity_remove))
        CollectionRowKind.BLOCKED -> listOf(
            "retry-revocation-1" to context().getString(R.string.settings_blocked_retry),
            "unblock-1" to context().getString(R.string.settings_unblock)
        )
        CollectionRowKind.PASSED -> listOf("unpass-1" to context().getString(R.string.settings_unpass))
        CollectionRowKind.VIEW -> listOf("received-view-open-1" to context().getString(R.string.profile_views_open))
    }

    private fun assertDate() {
        val date = compose.onNodeWithTag("received-view-date-1")
        date.performScrollTo().assertIsDisplayed()
            .assertTextEquals(context().getString(R.string.profile_views_date, "2024-02-29"))
        assertTrue("Every received-date character must fit", date.textLayout().textFitsLayout())
        val bounds = date.getUnclippedBoundsInRoot()
        val identifier = compose.onNodeWithText(COMPLETE_COLLECTION_PEER, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        assertTrue("Date follows the full identifier", bounds.top >= identifier.bottom)
    }

    private fun assertAction(tag: String, text: String) {
        val node = compose.onNode(hasText(text) and hasAnyAncestor(hasTestTag(tag)), useUnmergedTree = true)
        node.performScrollTo().assertIsDisplayed()
        assertTrue("Every action character must fit", node.textLayout().textFitsLayout())
        val control = node.getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("collection-geometry").getUnclippedBoundsInRoot()
        val id = compose.onNodeWithText(COMPLETE_COLLECTION_PEER, useUnmergedTree = true).getUnclippedBoundsInRoot()
        assertTrue("Actual viewport must have width", viewport.width.value > 0f)
        assertTrue("Action stays inside row", control.left >= viewport.left && control.right <= viewport.right)
        assertTrue("Action follows identifier without overlap", control.top >= id.bottom)
    }
}
