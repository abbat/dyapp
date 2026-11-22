package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileDeviceReadState
import uniffi.dyapp.MobileLinkCodeStatus
import java.util.concurrent.CountDownLatch

/** Admitted native-shaped completions must reveal feedback without test-driven scrolling after them. */
class LinkApprovalOutcomeTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishCompletionRevealsFeedbackAtBothActualTextScales() = verify("en")

    @Test
    fun russianCompletionRevealsFeedbackAtBothActualTextScales() = verify("ru")

    private fun verify(language: String) {
        val host = OwnerVisualHost(compose)
        val states = listOf(
            OwnerVisualState.SENT,
            OwnerVisualState.UNCERTAIN_EXPIRED,
            OwnerVisualState.UNCERTAIN_CHECK_ERROR
        )
        for (font in listOf(1f, 2f)) {
            for (state in states) {
                val scene = OwnerVisualScene(OwnerVisualBackend(state), ownerVisualContext(language, font), false, font)
                checkCompletion(host, scene)
            }
        }
    }

    private fun checkCompletion(host: OwnerVisualHost, scene: OwnerVisualScene) {
        val backend = scene.backend
        val release = CountDownLatch(1)
        backend.sending.set {
            backend.entered.complete(Unit)
            release.await()
        }
        try {
            host.show(scene)
            prepare(backend)
            NativeVisualGeometry(compose, "owner-dialog").density(scene.fontScale)
            compose.onNodeWithTag("owner-send").performClick()
            compose.waitUntil(5_000) { backend.entered.isCompleted }
            compose.onNodeWithTag("owner-cancel").assertIsNotEnabled()
            release.countDown()
            compose.awaitSnapshot(backend.approval.state, 5_000, "actual owner send completion") {
                !it.busy && it.delivery != null
            }
            val tag = checkResult(scene)
            if (scene.fontScale == 2f) {
                NativeVisualGeometry(compose, "owner-dialog").first()
                assertFalse("Current geometry must reject a hidden outcome", feedbackVisible(tag))
            }
        } finally {
            release.countDown()
            try { host.hide(scene) } finally { backend.close() }
        }
    }

    private fun checkResult(scene: OwnerVisualScene): String {
        val backend = scene.backend
        val sent = backend.state == OwnerVisualState.SENT
        val tag = if (sent) "owner-sent" else "owner-uncertain"
        compose.awaitNode(hasTestTag(if (sent) "owner-done" else "owner-cancel") and isFocused())
        awaitFeedback(tag)
        assertEquals(1, backend.sent.get())
        if (!sent) {
            if (scene.fontScale == 2f) checkFastRetry(backend)
            checkRetry(backend)
        }
        assertEquals(OWNER_VISUAL_CODE, backend.approval.state.value.opening?.code)
        assertTrue(compose.onNodeWithTag(tag).textLayout().textFitsLayout())
        return tag
    }

    private fun prepare(backend: OwnerVisualBackend) {
        compose.awaitSnapshot(backend.directory.state, 5_000, "owner directory") {
            !it.busy && it.listing?.state == MobileDeviceReadState.VERIFIED
        }
        compose.awaitNode(hasTestTag("owner-open") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        compose.onNodeWithTag("owner-code-input").performTextInput(OWNER_VISUAL_CODE)
        compose.onNodeWithTag("owner-code-next").performClick()
        compose.awaitSnapshot(backend.approval.state, 5_000, "owner comparison") {
            !it.busy && it.opening != null
        }
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        NativeVisualGeometry(compose, "owner-dialog").first()
    }

    private fun checkFastRetry(backend: OwnerVisualBackend) {
        NativeVisualGeometry(compose, "owner-dialog").first()
        assertFalse("Repeated feedback begins outside the current viewport", feedbackVisible("owner-uncertain"))
        val callback = compose.onNodeWithTag("owner-retry").fetchSemanticsNode().config[applicationClickKey]
        // This IO-only callback finishes before Compose can render SEND; identical text is not a new-result signal.
        compose.runOnIdle { runBlocking { completeRetry(backend, callback) } }
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        awaitFeedback("owner-uncertain")
        assertEquals(2, backend.sent.get())
    }

    private suspend fun completeRetry(backend: OwnerVisualBackend, callback: () -> Unit) {
        val previous = backend.approval.state.value.delivery
        withTimeout(5_000) {
            callback()
            backend.approval.state.first { !it.busy && it.delivery !== previous }
        }
    }

    private fun checkRetry(backend: OwnerVisualBackend) {
        val sent = backend.sent.get()
        val expired = backend.state == OwnerVisualState.UNCERTAIN_EXPIRED
        backend.inspect.set {
            if (expired) MobileLinkCodeStatus.EXPIRED else error("Controlled current-code read failure")
        }
        compose.onNodeWithTag("owner-retry").performClick()
        compose.awaitSnapshot(backend.approval.state, 5_000, "owner retry refusal") {
            !it.busy && it.codeStatus != MobileLinkCodeStatus.VALID
        }
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        awaitFeedback("owner-uncertain")
        if (expired) {
            compose.onNodeWithTag("owner-retry").assertDoesNotExist()
            compose.onNodeWithTag("owner-expired").assertExists()
        } else {
            compose.onNodeWithTag("owner-retry").assertIsNotEnabled()
            compose.onNodeWithTag("owner-error").assertExists()
        }
        assertEquals(sent, backend.sent.get())
    }

    private fun awaitFeedback(tag: String) {
        compose.awaitNode(hasTestTag(tag))
        compose.waitUntil(10_000) { feedbackVisible(tag) }
        compose.onNodeWithTag("owner-recipient").assertTextEquals(OWNER_VISUAL_CODE)
    }

    private fun feedbackVisible(tag: String): Boolean {
        val value = compose.onNodeWithTag(tag)
        val node = value.fetchSemanticsNode()
        val layout = value.textLayout()
        val viewport = compose.onNodeWithTag("owner-body").fetchSemanticsNode().boundsInRoot
        val top = node.positionInRoot.y + layout.getLineTop(0)
        val bottom = node.positionInRoot.y + layout.getLineBottom(0)
        return top >= viewport.top && bottom <= viewport.bottom
    }
}
