package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkFailure
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileQrMatrix
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Real screen and owner with controlled native completion, never a fake enrollment claim. */
internal class LinkReceiveFixture(private val compose: ComposeContentTestRule) {
    val destination = "reserved-" + "d".repeat(128)
    val creates = AtomicInteger()
    val polls = AtomicInteger()
    val closes = AtomicInteger()
    val route = mutableStateOf(SettingsPane.RECEIVE)
    var source by mutableStateOf(newSource())
    var mounted by mutableStateOf(true)
    var reviews = 0
    var returned = 0
    val store = LinkReceiverStore { _, _ ->
        LinkReceiverSession {
            creates.incrementAndGet()
            calls()
        }
    }

    private fun newSource() = Opened.Ready(Slot("source", File("presentation-only"), SlotState.READY), "original-peer")

    private fun calls(): LinkReceiverCalls {
        val phase = AtomicReference(MobileLinkPhase.WAITING)
        val receipt = MobileLinkReceipt.Failed(MobileLinkFailure.Destination(destination, "controlled disk refusal"))
        return LinkReceiverCalls(
            LinkReceiverDisplay("public-test-code", 123uL, MobileQrMatrix(1, listOf(true))),
            { phase.get() },
            { phase.get() },
            {
                polls.incrementAndGet()
                phase.set(MobileLinkPhase.FAILED)
                phase.get()
            },
            { if (phase.get() == MobileLinkPhase.FAILED) receipt else null },
            { closes.incrementAndGet() }
        )
    }

    fun show() = compose.setContent {
        if (mounted) key(source) { Screen() }
    }

    @Composable
    private fun Screen() = MaterialTheme {
        SettingsFrame(route) {
            if (it == SettingsPane.RECEIVE) {
                LinkReceiveScreen(
                    LinkReceiveSource(source),
                    LinkReceiveNavigation(
                        { route.value = SettingsPane.ACCOUNT },
                        { returned++ },
                        { reviews++ }
                    ),
                    store,
                    { MobileLinkCodeStatus.VALID }
                )
            }
        }
    }

    fun start(): LinkReceiverOpening {
        compose.onNodeWithTag("link-receive-start").performScrollTo().performClick()
        compose.awaitNode(hasTestTag("link-receive-destination"))
        return checkNotNull(store.state.value)
    }

    fun close() = runBlocking {
        store.state.value?.let { opening ->
            withTimeout(5_000) { opening.session.state.first { !it.busy } }
            store.release(opening)
        }
    }
}
