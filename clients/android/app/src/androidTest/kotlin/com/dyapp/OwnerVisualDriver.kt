package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileDeviceReadState
import uniffi.dyapp.MobileLinkCodeStatus

/** Production UI actions establish every captured state; worker fields are never overwritten. */
internal class OwnerVisualDriver(private val compose: ComposeContentTestRule) {
    fun show(backend: OwnerVisualBackend) {
        awaitDirectory(backend)
        compose.awaitNode(hasTestTag("owner-open") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("owner-code-input"))
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        if (backend.state == OwnerVisualState.INPUT) return
        val input = if (backend.state == OwnerVisualState.MALFORMED) "not-a-link-code" else OWNER_VISUAL_CODE
        compose.onNodeWithTag("owner-code-input").performTextInput(input)
        compose.onNodeWithTag("owner-code-next").performClick()
        if (backend.state == OwnerVisualState.PREPARING) {
            awaitEntered(backend)
        } else {
            awaitPreparation(backend)
            sendIfNeeded(backend)
        }
    }

    private fun sendIfNeeded(backend: OwnerVisualBackend) {
        if (backend.state in setOf(
                OwnerVisualState.MALFORMED,
                OwnerVisualState.EXPIRED,
                OwnerVisualState.PREPARE_ERROR,
                OwnerVisualState.CONFIRM
            )
        ) {
            return
        }
        if (backend.state == OwnerVisualState.CONFIRM_EXPIRED) {
            backend.inspect.set { MobileLinkCodeStatus.EXPIRED }
        }
        compose.awaitNode(hasTestTag("owner-send") and isEnabled()).performClick()
        if (backend.state == OwnerVisualState.SENDING) {
            awaitEntered(backend)
            return
        }
        awaitSend(backend)
        retryIfNeeded(backend)
    }

    private fun retryIfNeeded(backend: OwnerVisualBackend) {
        when (backend.state) {
            OwnerVisualState.UNCERTAIN_EXPIRED -> backend.inspect.set { MobileLinkCodeStatus.EXPIRED }
            OwnerVisualState.UNCERTAIN_CHECK_ERROR -> backend.inspect.set { error("Controlled code-check failure") }
            else -> return
        }
        compose.awaitNode(hasTestTag("owner-retry") and isEnabled()).performClick()
        runBlocking {
            withTimeout(5_000) {
                backend.approval.state.first { !it.busy && it.codeStatus != MobileLinkCodeStatus.VALID }
            }
        }
    }

    private fun awaitDirectory(backend: OwnerVisualBackend) = runBlocking {
        withTimeout(5_000) {
            backend.directory.state.first { !it.busy && it.listing?.state == MobileDeviceReadState.VERIFIED }
        }
    }

    private fun awaitPreparation(backend: OwnerVisualBackend) = runBlocking {
        withTimeout(5_000) {
            backend.approval.state.first { !it.busy && (it.codeStatus != null || it.failure != null) }
        }
    }

    private fun awaitSend(backend: OwnerVisualBackend) = runBlocking {
        withTimeout(5_000) {
            backend.approval.state.first {
                !it.busy && (it.delivery != null || it.codeStatus != MobileLinkCodeStatus.VALID)
            }
        }
    }

    private fun awaitEntered(backend: OwnerVisualBackend) = runBlocking {
        withTimeout(5_000) { backend.entered.await() }
    }
}
