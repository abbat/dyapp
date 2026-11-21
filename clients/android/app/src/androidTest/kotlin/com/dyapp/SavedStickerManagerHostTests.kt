package com.dyapp

import android.net.Uri
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.activity.result.ActivityResultRegistryOwner
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.SavedStickerPackImport
import uniffi.dyapp.SavedStickerPackPage
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class SavedStickerManagerHostTests {
    @get:Rule
    val compose = createComposeRule()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val registry = SavedStickerPickerRegistry(context)
    private var owner by mutableStateOf<Any>(Any())
    private val saves = AtomicInteger()
    private val received = AtomicReference<ByteArray?>()
    private val calls = SavedStickerCalls(
        { SavedStickerPackPage(emptyList(), null, 0uL, 0uL) },
        { bytes ->
            saves.incrementAndGet()
            received.set(bytes.copyOf())
            SavedStickerPackImport(ByteArray(32), true)
        },
        { error("unexpected removal") }
    )

    private fun show() {
        val registryOwner = object : ActivityResultRegistryOwner {
            override val activityResultRegistry = registry
        }
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalActivityResultRegistryOwner provides registryOwner) {
                    SavedStickerManagerHost(owner, calls) { SavedStickerManagerButton() }
                }
            }
        }
    }

    private fun openAndPick(): Int {
        compose.onNodeWithTag("saved-sticker-open").performClick()
        compose.awaitNode(hasTestTag("saved-sticker-import") and isEnabled()).performScrollTo().performClick()
        return registry.requests.last()
    }

    private fun closeManager() {
        compose.onNodeWithTag("saved-sticker-close").performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag("saved-sticker-manager").fetchSemanticsNodes().isEmpty()
        }
    }

    @Test
    fun retiredPickerResultsCannotImportThroughAReopenedManager() {
        val old = File.createTempFile("sticker-old-", ".tar", context.cacheDir)
        val current = File.createTempFile("sticker-new-", ".tar", context.cacheDir)
        val bytes = byteArrayOf(7, 8, 9, 10)
        try {
            old.writeBytes(byteArrayOf(1, 2, 3))
            current.writeBytes(bytes)
            show()
            val retired = openAndPick()
            closeManager()
            val active = openAndPick()
            assertNotEquals(retired, active)
            compose.runOnIdle {
                registry.deliver(retired, Uri.fromFile(old))
                registry.deliver(active, Uri.fromFile(current))
            }
            compose.awaitNode(hasText(context.getString(R.string.sticker_manager_added)))
            assertEquals(1, saves.get())
            assertArrayEquals(bytes, received.get())
        } finally {
            check(old.delete())
            check(current.delete())
        }
    }

    @Test
    fun aRetiredIdentityOpeningCannotOpenTheSuccessorManager() {
        show()
        val oldOpen = compose.onNodeWithTag("saved-sticker-open").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { owner = Any() }
        compose.awaitNode(hasTestTag("saved-sticker-open"))
        compose.runOnIdle { oldOpen() }
        compose.onNodeWithTag("saved-sticker-manager").assertDoesNotExist()
        compose.onNodeWithTag("saved-sticker-open").performClick()
        compose.awaitNode(hasTestTag("saved-sticker-import") and isEnabled())
    }
}
