package com.dyapp

import android.content.ClipboardManager
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import uniffi.dyapp.MobileLinkCodeStatus

private const val OWNER_UI_STEP_MS = 180_000L

/** Actual consent and encrypted transfer through production widgets and their real native adapter. */
internal class NativeOwnerApprovalJourney(
    private val host: NativeLinkScreenHost,
    private val original: Opened.Ready
) : AutoCloseable {
    private val compose = host.compose
    private val directory = DeviceDirectorySession { nativeDeviceDirectoryCalls(original) }
    private val approval = nativeLinkApprovalSession(original)
    private val scene = NativeLinkScreenScene(original, directory, approval)

    fun open() {
        host.show(scene)
        host.openMenu(original)
        host.verifiedDirectory(scene)
        compose.awaitNode(hasTestTag("owner-open") and isEnabled())
    }

    fun approve(code: String) {
        enterCode(code)
        val opening = awaitPreparation(approval)
        assertEquals(code, opening.code)
        compose.awaitNode(hasTestTag("owner-recipient")).assertTextEquals(opening.code)
        compose.onNodeWithTag("owner-identity").assertTextEquals(original.peerId)
        checkTargetCopy(original.peerId, opening.code)
        compose.onNodeWithTag("owner-send").performClick()
        awaitDelivery(approval).result.getOrThrow()
        compose.awaitNode(hasTestTag("owner-sent")).assertTextEquals(text(R.string.owner_sent))
        compose.onNodeWithTag("owner-done").performClick()
    }

    private fun enterCode(code: String) {
        compose.awaitNode(hasTestTag("owner-open") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        compose.awaitNode(hasTestTag("owner-code-input")).performScrollTo().performTextInput(code)
        compose.onNodeWithTag("owner-code-next").performClick()
    }

    private fun awaitPreparation(session: LinkApprovalSession): LinkApprovalOpening {
        val view = compose.awaitSnapshot(session.state, OWNER_UI_STEP_MS, "owner preparation") {
            !it.busy && (it.opening != null || it.codeStatus != null || it.failure != null)
        }
        view.failure?.let { throw it }
        assertEquals(MobileLinkCodeStatus.VALID, view.codeStatus)
        return checkNotNull(view.opening)
    }

    private fun awaitDelivery(session: LinkApprovalSession): LinkApprovalDelivery {
        val view = compose.awaitSnapshot(session.state, OWNER_UI_STEP_MS, "owner delivery") {
            !it.busy && (it.delivery != null || it.codeStatus != MobileLinkCodeStatus.VALID || it.failure != null)
        }
        view.failure?.let { throw it }
        assertEquals(MobileLinkCodeStatus.VALID, view.codeStatus)
        return checkNotNull(view.delivery)
    }

    private fun checkTargetCopy(identity: String, code: String) {
        compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
        for ((tag, expected) in listOf("owner-identity" to identity, "owner-recipient" to code)) {
            compose.onNodeWithTag("copy-$tag").performScrollTo().performClick()
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            assertEquals(expected, clipboard.primaryClip?.getItemAt(0)?.text?.toString())
        }
        compose.onNodeWithTag("owner-consequences").performScrollTo()
            .assertTextEquals(text(R.string.owner_confirm_text))
        compose.onNodeWithTag("owner-possession").performScrollTo()
            .assertTextEquals(text(R.string.owner_confirm_warn))
    }

    override fun close() {
        try {
            host.hide(scene)
        } finally {
            runBlocking {
                try {
                    withTimeout(OWNER_UI_STEP_MS) { approval.retire().await().getOrThrow() }
                } finally {
                    withTimeout(OWNER_UI_STEP_MS) { directory.retire().await().getOrThrow() }
                }
            }
        }
    }

    private fun text(id: Int): String = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)
}
