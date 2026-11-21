package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test

class StickerPickerComposerTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun queuedStickerIsStandaloneAndPreservesTheActualComposersEditAndHeldDraft() {
        val fixture = StickerPickerUiFixture(compose)
        fixture.form.edit(fixture.messages.single())
        val editing = fixture.form.editing.value
        fixture.show()
        compose.onNodeWithTag("chat-input").performTextReplacement("edited but not sent")
        fixture.recent()
        compose.onNodeWithTag(fixture.choiceTag(fixture.wide)).performScrollTo().performClick()
        fixture.closed()
        assertEquals(1, fixture.sends.get())
        assertEquals(0, fixture.textSends.get())
        assertSame(fixture.wide, fixture.sent.get())
        assertEquals(editing, fixture.form.editing.value)
        assertEquals("draft survives", fixture.form.held.value)
        compose.onNodeWithTag("chat-input").assertTextEquals("edited but not sent")
    }

    @Test
    fun managerReturnCreatesFreshChoicesAndRetiredButtonsCannotSend() {
        val fixture = StickerPickerUiFixture(compose)
        fixture.show()
        fixture.recent()
        val retired = compose.onNodeWithTag(fixture.choiceTag(fixture.wide))
            .fetchSemanticsNode()
            .config[applicationClickKey]
        compose.onNodeWithTag("sticker-picker-manage").performClick()
        compose.awaitNode(hasTestTag("saved-sticker-import") and isEnabled())
        compose.onNodeWithTag("saved-sticker-close").performClick()
        compose.awaitNode(hasTestTag("sticker-picker-recent") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag(fixture.choiceTag(fixture.wide)) and isEnabled())
        compose.runOnIdle { retired() }
        assertEquals(0, fixture.sends.get())
        compose.onNodeWithTag(fixture.choiceTag(fixture.wide)).performScrollTo().performClick()
        fixture.closed()
        assertEquals(1, fixture.sends.get())
        compose.onNodeWithTag("chat-input").assertTextEquals("draft survives")
    }

    @Test
    fun aReplacedIdentityCannotUseThePreviousPickersSendCallback() {
        val fixture = StickerPickerUiFixture(compose)
        fixture.show()
        fixture.recent()
        val retired = compose.onNodeWithTag(fixture.choiceTag(fixture.wide))
            .fetchSemanticsNode()
            .config[applicationClickKey]
        compose.runOnIdle { fixture.owner.value = Any() }
        fixture.closed()
        fixture.recent()
        compose.runOnIdle { retired() }
        assertEquals(0, fixture.sends.get())
        compose.onNodeWithTag("sticker-picker-cancel").performClick()
        fixture.closed()
        compose.onNodeWithTag("chat-input").assertTextEquals("draft survives")
    }
}
