package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.NetworkFile
import java.io.File

/** Controls permission completion, while retaining the real form and native identities. */
class NetworkPermissionOwnerTests {
    @get:Rule
    val compose = createComposeRule()
    private var owner by mutableStateOf<Opened.Ready?>(null)
    private val pending = ArrayDeque<() -> Unit>()

    private fun show(opened: Opened.Ready) {
        owner = opened
        compose.setContent {
            CompositionLocalProvider(LocalNetworkRequest provides { action -> pending.addLast(action) }) {
                val current = owner
                if (current == null) {
                    Text("closed", Modifier.testTag("permission-owner-closed"))
                } else {
                    NetworkDraftScreen(current) {}
                }
            }
        }
        ready(opened)
    }

    private fun ready(opened: Opened.Ready) {
        compose.onNodeWithTag("peer-id").assertTextEquals(opened.peerId)
        compose.awaitNode(hasTestTag("check-connection") and isEnabled())
        compose.onNodeWithTag("seeds").performTextReplacement("not an address")
    }

    private fun request() {
        compose.onNodeWithTag("check-connection").performSemanticsAction(SemanticsActions.OnClick)
        compose.runOnIdle { assertEquals(1, pending.size) }
        compose.onNodeWithTag("network-check-status").assertDoesNotExist()
    }

    private fun complete() = compose.runOnIdle { pending.removeFirst().invoke() }

    private fun closeScreen() {
        compose.runOnIdle { owner = null }
        compose.onNodeWithTag("permission-owner-closed").assertExists()
    }

    @Test
    fun replacementAndDisposalCannotRedirectPendingPermissionWork() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val second = fixture.slots.create()
            fixture.slots.choose(first.slot.id)
            try {
                show(first)
                request()
                fixture.slots.choose(second.slot.id)
                compose.runOnIdle { owner = second }
                ready(second)
                complete()
                compose.onNodeWithTag("network-check-status").assertDoesNotExist()
                assertFalse(File(first.slot.dir, "network-draft").exists())
                assertFalse(File(second.slot.dir, "network-draft").exists())
                checkCurrentCompletion(second)
                request()
                val draft = RustBridge.loadNetwork(second, NetworkFile.DRAFT)
                closeScreen()
                complete()
                assertEquals(draft, RustBridge.loadNetwork(second, NetworkFile.DRAFT))
                assertEquals(second.peerId, (fixture.slots.open() as Opened.Ready).peerId)
                assertNull(RustBridge.loadNetwork(first, NetworkFile.ACTIVE))
                assertNull(RustBridge.loadNetwork(second, NetworkFile.ACTIVE))
            } finally {
                closeScreen()
            }
        }
    }

    private fun checkCurrentCompletion(opened: Opened.Ready) {
        request()
        complete()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val invalid = context.getString(R.string.network_invalid_address)
        compose.awaitNode(hasTestTag("network-check-status") and hasText(invalid))
        assertEquals(listOf("not an address"), RustBridge.loadNetwork(opened, NetworkFile.DRAFT)?.seeds)
        // Editing resets the displayed result before the next permission request.
        compose.onNodeWithTag("seeds").performTextReplacement("still not an address")
    }
}
