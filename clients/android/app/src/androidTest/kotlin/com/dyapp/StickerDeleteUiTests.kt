package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class StickerDeleteUiTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun cancelAndReopenRetireTheOldConfirmationAndOnlyTheSelectedStickerIsSubmitted() {
        val fixture = StickerDeleteUiFixture(compose)
        fixture.show()
        fixture.ask()
        val old = compose.onNodeWithTag("sticker-delete-confirm").fetchSemanticsNode().config[applicationClickKey]
        compose.onNodeWithTag("sticker-delete-cancel").performClick()
        fixture.closed()
        fixture.ask()
        compose.runOnIdle { old() }
        assertEquals(0, fixture.deletes.get())
        compose.onNodeWithTag("sticker-delete-confirm").performClick()
        compose.awaitNode(hasTestTag("sticker-delete-status"))
        assertEquals(1, fixture.deletes.get())
        val selected = checkNotNull(fixture.selected.get())
        assertEquals(fixture.own.author, selected.author)
        assertArrayEquals(fixture.own.groupId, selected.groupId)
        assertArrayEquals(fixture.own.id, selected.id)
        compose.onNodeWithTag("chat-input").assertTextEquals("keep the draft")
    }

    @Test
    fun ownStickerMenuHasOnlyDeleteAndACommittedResultCannotBeSubmittedAgain() {
        val fixture = StickerDeleteUiFixture(compose)
        fixture.show()
        compose.onAllNodesWithTag("own-sticker-menu-target").assertCountEquals(1)
        compose.onNodeWithTag("own-sticker-menu-target").performSemanticsAction(SemanticsActions.OnLongClick) { it() }
        compose.awaitNode(hasTestTag("sticker-message-menu"))
        compose.onAllNodes(hasAnyAncestor(hasTestTag("sticker-message-menu")) and hasClickAction()).assertCountEquals(1)
        compose.onNodeWithTag("sticker-message-delete").performClick()
        compose.awaitNode(hasTestTag("sticker-delete-confirm")).performClick()
        compose.awaitNode(hasTestTag("sticker-delete-status"))
        compose.onNodeWithTag("own-sticker-menu-target").performSemanticsAction(SemanticsActions.OnLongClick) { it() }
        compose.awaitNode(hasTestTag("sticker-message-delete")).assertIsNotEnabled()
        assertEquals(1, fixture.deletes.get())
    }

    @Test
    fun replacingTheIdentityRetiresAnAlreadyCapturedConfirmation() {
        val fixture = StickerDeleteUiFixture(compose)
        fixture.show()
        fixture.ask()
        val old = compose.onNodeWithTag("sticker-delete-confirm").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { fixture.owner.value = Any() }
        fixture.closed()
        compose.runOnIdle { old() }
        assertEquals(0, fixture.deletes.get())
        fixture.ask()
        compose.onNodeWithTag("sticker-delete-cancel").performClick()
        fixture.closed()
        compose.onNodeWithTag("chat-input").assertTextEquals("keep the draft")
    }
}
