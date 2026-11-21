package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.SavedStickerPackState
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class SavedStickerManagerViewTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun fullPackIdentityAndCancelGuardConfirmedRemoval() {
        val fixture = SavedStickerViewFixture()
        fixture.show(compose)
        val second = "00".repeat(31) + "02"
        fun ask() = compose.awaitNode(hasTestTag("saved-sticker-remove-" + second)).performScrollTo().performClick()
        ask()
        compose.awaitNode(hasTestTag("saved-sticker-remove-id")).assertTextEquals(second)
        val retired = compose.onNodeWithTag("saved-sticker-confirm")
            .fetchSemanticsNode()
            .config[applicationClickKey]
        compose.onNodeWithTag("saved-sticker-cancel").performClick()
        assertEquals(0, fixture.removals.get())
        ask()
        compose.runOnIdle { retired() }
        assertEquals(0, fixture.removals.get())
        compose.onNodeWithTag("saved-sticker-confirm").performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag("saved-sticker-row-" + second).fetchSemanticsNodes().isEmpty()
        }
        assertEquals(1, fixture.removals.get())
        assertArrayEquals(fixture.second.packId, fixture.removed.get())
        compose.onNodeWithTag("saved-sticker-manager").assertExists()
        compose.onNodeWithTag("saved-sticker-row-" + "00".repeat(31) + "01").assertExists()
    }

    @Test
    fun nextUsesTheCoreCursorAndRetiredPageCallbacksCannotReadAgain() {
        val fixture = SavedStickerViewFixture(paged = true)
        fixture.show(compose)
        val next = compose.awaitNode(hasTestTag("saved-sticker-next")).performScrollTo()
        val retired = next.fetchSemanticsNode().config[applicationClickKey]
        next.performClick()
        compose.awaitNode(hasTestTag("saved-sticker-row-" + "00".repeat(31) + "02"))
        compose.runOnIdle { retired() }
        assertEquals(2, fixture.reads.get())
        compose.onNodeWithTag("saved-sticker-refresh").performScrollTo().performClick()
        compose.awaitNode(hasTestTag("saved-sticker-row-" + "00".repeat(31) + "01"))
        assertEquals(3, fixture.reads.get())
    }
}

internal class SavedStickerViewFixture(private val paged: Boolean = false) {
    val scale = mutableStateOf(1f)
    val name = "Saved pack <plain text> & long name ".repeat(4)
    private val first = entry(1)
    val second = entry(2)
    val removals = AtomicInteger()
    val reads = AtomicInteger()
    val removed = AtomicReference<ByteArray?>()
    private val cursor = ByteArray(32) { 9 }
    private val calls = SavedStickerCalls(
        page = { after ->
            reads.incrementAndGet()
            if (after != null) assertArrayEquals(cursor, after)
            val entries = if (paged) listOf(if (after == null) first else second) else listOf(first, second)
            SavedStickerPackPage(
                entries.filterNot { removed.get()?.contentEquals(it.packId) == true },
                if (paged && after == null) cursor else null,
                38uL,
                0uL
            )
        },
        save = { error("unexpected import") },
        remove = { id ->
            removals.incrementAndGet()
            removed.set(id.copyOf())
            true
        }
    )
    private val state = SavedStickerPresentation().apply { runBlocking { refresh(calls) } }

    private fun entry(last: Byte) = SavedStickerPackEntry(
        ByteArray(32).also { it[it.lastIndex] = last },
        19uL,
        if (last == 1.toByte()) {
            SavedStickerPackState.Ready(name, 1u)
        } else {
            SavedStickerPackState.Unreadable("not a display label")
        }
    )

    fun show(compose: ComposeContentTestRule, language: String = "en"): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent { Content(context) }
        return context
    }

    @Composable
    private fun Content(context: Context) {
        LocalizedTestContext(context, scale.value) {
            Manager()
        }
    }

    @Composable
    private fun Manager() {
        val scope = rememberCoroutineScope()
        val actions = SavedStickerActions(
            pick = {},
            navigate = { after, shown ->
                scope.launch(start = CoroutineStart.UNDISPATCHED) { state.refresh(calls, after, shown) }
            },
            confirm = { shown ->
                scope.launch(start = CoroutineStart.UNDISPATCHED) { state.confirmRemove(calls, shown) }
            },
            close = {}
        )
        DYAppTheme { SavedStickerManagerView(state, actions) }
    }
}
