package com.dyapp

import androidx.compose.ui.test.onNodeWithText
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.ChatState

internal class ChatReadEffectsTests : ChatReadFixture() {
    @Test
    fun refreshDoesNotCancelAReadAndNetworkAcknowledgementDoesNotDelayEitherSnapshot() {
        val first = Gate()
        val second = Gate()
        val acknowledgement = Acknowledgement()
        val selection = Selection()
        var reads = 0
        show(selection, { if (reads++ == 0) first.read() else second.read() }, acknowledgement::run)
        try {
            refreshDuringRead(selection, first, second)
            await(acknowledgement.entered)
            compose.onNodeWithText(ChatState.REQUEST_RECEIVED.name).assertExists()
            second.released.complete(snapshot(ChatState.OPEN))
            await(second.returned)
            compose.onNodeWithText(ChatState.OPEN.name).assertExists()
            compose.runOnIdle { assertEquals(1, acknowledgement.calls) }
            acknowledgement.released.complete(Unit)
            await(acknowledgement.again)
            compose.runOnIdle { assertEquals(2, reads) }
        } finally {
            release(first, second, acknowledgement)
        }
    }

    @Test
    fun aCancelledOwnersLateNativeReadCannotPublishOrAcknowledgeThroughNewCallbacks() {
        val old = Gate()
        val acknowledged = CompletableDeferred<Unit>()
        val selection = Selection()
        show(selection, { if (selection.owner == "old") old.read() else snapshot(ChatState.OPEN) }) {
            acknowledged.complete(Unit)
        }
        try {
            await(old.entered)
            compose.runOnIdle { selection.owner = "new" }
            compose.onNodeWithText(ChatState.OPEN.name).assertExists()
            await(acknowledged)
            old.released.complete(snapshot(ChatState.REQUEST_RECEIVED))
            await(old.returned)
            compose.runOnIdle {
                assertEquals(listOf(ChatState.OPEN), selection.published)
                assertEquals(listOf("new"), selection.acknowledgementOwners)
            }
            compose.onNodeWithText(ChatState.REQUEST_RECEIVED.name).assertDoesNotExist()
        } finally {
            old.released.complete(snapshot(ChatState.OPEN))
        }
    }

    @Test
    fun failedLocalReadsReportFailureAndOnlyASuccessfulRefreshTriggersAcknowledgement() {
        val acknowledged = CompletableDeferred<Unit>()
        val selection = Selection()
        show(selection, {
            if (selection.revision == 0) error("controlled local read failure")
            snapshot(ChatState.OPEN)
        }) { acknowledged.complete(Unit) }
        compose.onNodeWithText("failed").assertExists()
        compose.runOnIdle { assertEquals(false, acknowledged.isCompleted) }
        compose.runOnIdle { selection.revision += 1 }
        compose.onNodeWithText(ChatState.OPEN.name).assertExists()
        await(acknowledged)
    }
}
