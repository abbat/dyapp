package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performSemanticsAction
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatNotice

private class PendingNoticeWrite(val owner: String, val content: ChatNotice) {
    val result = CompletableDeferred<Boolean>()
    val returned = CompletableDeferred<Unit>()

    suspend fun await(): Boolean = try {
        result.await()
    } finally {
        returned.complete(Unit)
    }
}

/** The real preference card, with native write completion controlled independently of UI timing. */
class NoticeCardOwnerTests {
    @get:Rule
    val compose = createComposeRule()
    private var owner by mutableStateOf<Opened.Ready?>(null)
    private val writes = mutableListOf<PendingNoticeWrite>()

    private fun show(active: Opened.Ready) {
        owner = active
        compose.setContent {
            val current = owner
            if (current == null) {
                Text("closed", Modifier.testTag("notice-owner-closed"))
            } else {
                NoticeCard(current, read = { true to ChatNotice.TEXT }, write = { target, content ->
                    val pending = PendingNoticeWrite(target.slot.id, content)
                    writes.add(pending)
                    pending.await()
                })
            }
        }
        ready()
    }

    private fun ready() = compose.awaitNode(hasTestTag("message-notifications") and isEnabled())

    private fun click(tag: String) = compose.onNodeWithTag(tag).performSemanticsAction(SemanticsActions.OnClick)

    private fun pending(index: Int): PendingNoticeWrite {
        compose.runOnIdle { assertEquals(index + 1, writes.size) }
        return writes[index]
    }

    private fun assertPendingControls() {
        compose.onNodeWithTag("message-notifications").assertIsNotEnabled()
        listOf("notice-fixed", "notice-name", "notice-text").forEach {
            compose.onNodeWithTag(it).assertIsNotEnabled()
        }
    }

    private fun close() {
        compose.runOnIdle { owner = null }
        compose.onNodeWithTag("notice-owner-closed").assertExists()
    }

    @Test
    fun pendingSaveRejectsRepeatedAdmissionAndFailureRestoresControls() {
        NativeSlotFixture().use { fixture ->
            try {
                show(fixture.slots.create())
                val choice = compose.onNodeWithTag("notice-name").fetchSemanticsNode().config[SemanticsActions.OnClick]
                click("notice-name")
                val first = pending(0)
                assertPendingControls()
                // Even a previously captured semantics action cannot bypass admission while pending.
                compose.runOnIdle { choice.action?.invoke() }
                pending(0)
                assertEquals(ChatNotice.NAME, first.content)
                compose.runOnIdle { first.result.complete(false) }
                ready()
                compose.onNodeWithTag("notice-error").assertExists()
                compose.onNodeWithTag("notice-text").assertIsSelected()
                click("message-notifications")
                val retry = pending(1)
                assertEquals(ChatNotice.OFF, retry.content)
                compose.runOnIdle { retry.result.complete(true) }
                ready()
                compose.onNodeWithTag("message-notifications").assertIsOff()
                compose.onNodeWithTag("notice-text").assertIsSelected().assertIsNotEnabled()
                compose.onNodeWithTag("notice-error").assertDoesNotExist()
            } finally {
                close()
            }
        }
    }

    @Test
    fun replacementAndDisposalCancelTheOldPresentationContinuation() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val second = fixture.slots.create()
            fixture.slots.choose(first.slot.id)
            try {
                show(first)
                click("message-notifications")
                val old = pending(0)
                assertEquals(first.slot.id, old.owner)
                compose.runOnIdle { owner = second }
                ready()
                compose.waitUntil(5_000) { old.returned.isCompleted }
                compose.runOnIdle { old.result.complete(true) }
                compose.onNodeWithTag("message-notifications").assertIsOn()
                compose.onNodeWithTag("notice-text").assertIsSelected()
                compose.onNodeWithTag("notice-error").assertDoesNotExist()
                click("message-notifications")
                val disposed = pending(1)
                assertEquals(second.slot.id, disposed.owner)
                close()
                compose.waitUntil(5_000) { disposed.returned.isCompleted }
                compose.runOnIdle { disposed.result.complete(true) }
                compose.onNodeWithTag("notice-owner-closed").assertExists()
            } finally {
                close()
            }
        }
    }
}
