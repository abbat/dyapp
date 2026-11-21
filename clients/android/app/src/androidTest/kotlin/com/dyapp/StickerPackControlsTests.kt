package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.StickerPackSaveAttempt
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus
import java.util.concurrent.atomic.AtomicInteger

class StickerPackControlsTests {
    @get:Rule
    val compose = createComposeRule()

    private fun status(state: StickerPackSaveState, control: StickerPackSaveControl) = StickerPackSaveStatus(
        state = state,
        generation = if (state == StickerPackSaveState.WAITING) ByteArray(16) else null,
        failure = null,
        retryAtMs = null,
        controls = listOf(control)
    )

    private fun text(resource: Int, name: String = "Pack") {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(hasText(context.getString(resource, name)))
    }

    private fun show(target: () -> StickerPackTarget, manage: () -> Unit = {}) {
        compose.setContent { MaterialTheme { StickerPackControls(target(), manage) } }
    }

    @Test
    fun projectedControlsAndRetiredOwnersDetermineNavigation() {
        var owner by mutableStateOf("first")
        val opens = AtomicInteger()
        val native = status(StickerPackSaveState.NEW, StickerPackSaveControl.MANAGE)
        val writes = AtomicInteger()
        val calls = StickerPackCalls({ native }, { _, _ ->
            writes.incrementAndGet()
            error("unexpected mutation")
        })
        show({ StickerPackTarget(owner, owner, calls) }, { opens.incrementAndGet() })
        text(R.string.sticker_pack_new, "first")
        compose.onNodeWithTag("sticker-pack-REQUEST").assertDoesNotExist()
        val button = compose.awaitNode(hasTestTag("sticker-pack-MANAGE") and isEnabled())
        val retired = button.fetchSemanticsNode().config[applicationClickKey]
        button.performClick()
        assertEquals(1, opens.get())
        compose.runOnIdle { owner = "second" }
        text(R.string.sticker_pack_new, "second")
        compose.runOnIdle { retired() }
        assertEquals(1, opens.get())
        compose.onNodeWithTag("sticker-pack-MANAGE").performClick()
        assertEquals(2, opens.get())
        assertEquals(0, writes.get())
    }

    @Test
    fun uncertainApplicationRetainsWaitingAndOffersOnlyReadOnlyRecovery() {
        val waiting = status(StickerPackSaveState.WAITING, StickerPackSaveControl.STOP_WAITING)
        var current: StickerPackSaveStatus? = waiting
        val writes = AtomicInteger()
        val calls = StickerPackCalls({ checkNotNull(current) }, { _, _ ->
            writes.incrementAndGet()
            current = null
            StickerPackSaveAttempt.Unavailable("controlled post-commit read failure")
        })
        show({ StickerPackTarget("owner", "Pack", calls) })
        text(R.string.sticker_pack_waiting)
        val button = compose.awaitNode(hasTestTag("sticker-pack-STOP_WAITING") and isEnabled())
        val retired = button.fetchSemanticsNode().config[applicationClickKey]
        button.performClick()
        text(R.string.sticker_pack_read_failed)
        text(R.string.sticker_pack_waiting)
        compose.onNodeWithTag("sticker-pack-STOP_WAITING").assertDoesNotExist()
        compose.runOnIdle { retired() }
        assertEquals(1, writes.get())
        current = status(StickerPackSaveState.SAVED, StickerPackSaveControl.MANAGE)
        compose.onNodeWithTag("sticker-pack-REFRESH").performClick()
        text(R.string.sticker_pack_saved)
        assertEquals(1, writes.get())
    }
}
