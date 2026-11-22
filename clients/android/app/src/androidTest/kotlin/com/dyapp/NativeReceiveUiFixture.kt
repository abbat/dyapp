package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileLinkReceipt

/** Production receiving controls, store, native factory and polling; no simulated receipt. */
internal class NativeReceiveUiFixture(
    private val compose: ComposeContentTestRule,
    private val source: Opened.Ready
) : AutoCloseable {
    private val mounted = mutableStateOf(true)
    private val route = mutableStateOf(SettingsPane.RECEIVE)
    val store = LinkReceiverStore { opened, wizard ->
        LinkReceiverSession { linkReceiverCalls(opened, wizard) }
    }

    init {
        compose.setContent {
            if (mounted.value) {
                MaterialTheme {
                    SettingsFrame(route) {
                        if (it == SettingsPane.RECEIVE) {
                            LinkReceiveScreen(
                                LinkReceiveSource(source),
                                LinkReceiveNavigation(
                                    { route.value = SettingsPane.ACCOUNT },
                                    { route.value = SettingsPane.ACCOUNT },
                                    {}
                                ),
                                store
                            )
                        }
                    }
                }
            }
        }
    }

    fun start(): LinkReceiverDisplay {
        compose.onNodeWithTag("link-receive-start").performScrollTo().performClick()
        val created = await { it.display != null }
        val display = checkNotNull(created.display)
        compose.awaitNode(hasTestTag("link-receive-code"))
        return display
    }

    fun installed(): MobileLinkReceipt.Installed {
        val result = await { !it.busy && it.receipt != null }
        val receipt = result.receipt
        check(receipt is MobileLinkReceipt.Installed) { "Native receiving did not install a profile" }
        compose.awaitNode(hasTestTag("link-receive-peer"))
        return receipt
    }

    fun finishEnrollment() {
        compose.awaitNode(hasTestTag("link-setup-finish")).performScrollTo().performClick()
        val result = await { !it.busy && (it.enrollment.completed || it.enrollment.failure != null) }
        result.enrollment.failure?.let { throw it }
        check(result.enrollment.completed)
    }

    private fun await(ready: (LinkReceiverSnapshot) -> Boolean): LinkReceiverSnapshot = runBlocking {
        val session = checkNotNull(store.state.value).session
        val result = withTimeout(NATIVE_LINK_UI_STEP_MS) {
            session.state.first { it.error != null || it.receipt is MobileLinkReceipt.Failed || ready(it) }
        }
        result.error?.let { throw it }
        check(result.receipt !is MobileLinkReceipt.Failed) { "Native receiving failed" }
        result
    }

    override fun close() {
        try {
            compose.runOnIdle { mounted.value = false }
        } finally {
            store.state.value?.let { opening ->
                opening.session.retire()
                runBlocking {
                    withTimeout(NATIVE_LINK_UI_STEP_MS) {
                        opening.session.state.first { !it.busy }
                        check(store.release(opening))
                    }
                }
            }
        }
    }
}
