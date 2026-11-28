package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
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
        val editor = hasTestTag("owner-code-input")
        compose.awaitNode(editor and isEnabled()).performTextInput(input)
        compose.awaitNode(editor and hasText(input))
        compose.awaitNode(hasTestTag("owner-code-next") and isEnabled()).performClick()
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
        activate("owner-send")
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
        activate("owner-retry")
        compose.awaitSnapshot(backend.approval.state, 5_000, "owner retry") {
            !it.busy && it.codeStatus != MobileLinkCodeStatus.VALID
        }
    }

    private fun activate(tag: String) {
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        compose.awaitNode(hasTestTag(tag) and isEnabled()).performScrollTo().performClick()
    }

    private fun awaitDirectory(backend: OwnerVisualBackend) =
        compose.awaitSnapshot(backend.directory.state, 5_000, "owner directory") {
            !it.busy && it.listing?.state == MobileDeviceReadState.VERIFIED
        }

    private fun awaitPreparation(backend: OwnerVisualBackend) =
        compose.awaitSnapshot(backend.approval.state, 5_000, "owner preparation") {
            !it.busy && (it.codeStatus != null || it.failure != null)
        }

    private fun awaitSend(backend: OwnerVisualBackend) =
        compose.awaitSnapshot(backend.approval.state, 5_000, "owner delivery ${backend.state}") {
            !it.busy && (it.delivery != null || it.codeStatus != MobileLinkCodeStatus.VALID)
        }

    private fun awaitEntered(backend: OwnerVisualBackend) {
        compose.waitUntil(5_000) { backend.entered.isCompleted }
    }
}
