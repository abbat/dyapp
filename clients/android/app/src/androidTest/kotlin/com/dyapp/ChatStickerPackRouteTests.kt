package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatStickerImage
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.StickerPackSaveAttempt
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class ChatStickerPackRouteTests {
    @get:Rule
    val compose = createComposeRule()
    private val owner = Any()
    private var revision by mutableIntStateOf(0)
    private val writes = AtomicInteger()
    private val current = AtomicReference(status(StickerPackSaveState.NEW, StickerPackSaveControl.MANAGE))
    private val sticker = ChatSticker(
        byteArrayOf(1), "sender", ByteArray(16), 1uL, 1uL, ByteArray(32), ByteArray(32), ":)", "Pack", false, null
    )
    private val calls = StickerPackCalls({ current.get() }, { shown, control ->
        writes.incrementAndGet()
        assertSame(current.get(), shown)
        assertEquals(StickerPackSaveControl.STOP_WAITING, control)
        current.set(status(StickerPackSaveState.NEW, StickerPackSaveControl.MANAGE))
        StickerPackSaveAttempt.Ready(current.get())
    })

    private fun status(state: StickerPackSaveState, control: StickerPackSaveControl) =
        StickerPackSaveStatus(state, ByteArray(16), null, null, listOf(control))

    private fun show() {
        val manager = SavedStickerCalls({
            current.set(status(StickerPackSaveState.SAVED, StickerPackSaveControl.MANAGE))
            SavedStickerPackPage(emptyList(), null, 0uL, 0uL)
        }, { error("unexpected import") }, { error("unexpected removal") })
        compose.setContent {
            MaterialTheme {
                SavedStickerManagerHost(owner, manager) {
                    val source = ChatStickerSource(
                        owner,
                        revision,
                        { StickerPackTarget(owner, it.packName, calls, revision) }
                    ) { _, _ -> ChatStickerImage.Unavailable }
                    CompositionLocalProvider(LocalChatStickerSource provides source) {
                        Column(Modifier.verticalScroll(rememberScrollState())) { ChatStickerBubble(sticker) }
                    }
                }
            }
        }
    }

    private fun caption(resource: Int) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(hasTestTag("sticker-pack-caption") and hasText(context.getString(resource, "Pack")))
    }

    @Test
    fun chatManageOpensTheRealDialogAndClosingReconcilesTheClickedPack() {
        show()
        caption(R.string.sticker_pack_new)
        compose.awaitNode(hasTestTag("sticker-pack-MANAGE") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("saved-sticker-import") and isEnabled())
        compose.onNodeWithTag("saved-sticker-close").performClick()
        caption(R.string.sticker_pack_saved)
        assertEquals(0, writes.get())
        compose.onNodeWithTag("sticker-pack-REQUEST").assertDoesNotExist()
    }

    @Test
    fun mailboxRevisionRefreshesTheSameMessageAndUsesItsExactStatusForStopWaiting() {
        show()
        caption(R.string.sticker_pack_new)
        compose.runOnIdle {
            current.set(status(StickerPackSaveState.WAITING, StickerPackSaveControl.STOP_WAITING))
            revision += 1
        }
        caption(R.string.sticker_pack_waiting)
        compose.awaitNode(hasTestTag("sticker-pack-STOP_WAITING") and isEnabled()).performScrollTo().performClick()
        caption(R.string.sticker_pack_new)
        assertEquals(1, writes.get())
    }
}
