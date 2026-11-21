package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission

class ProfileShareOpeningTests {
    @get:Rule
    val compose = createComposeRule()
    private val form = ProfileShareForm().apply { shown = true }
    private val peer = "a".repeat(64)
    private val sent = mutableListOf<String>()
    private val opened = mutableListOf<String>()
    private var mounted by mutableStateOf(true)
    private val row = ChatRow(
        peerId = peer,
        state = ChatState.OPEN,
        lastMessage = null,
        preview = ChatListPreview(ChatPreviewBody.Empty, false, null),
        unreadCount = 0uL,
        muted = false,
    )

    private fun show() = compose.setContent {
        if (mounted && form.shown) ProfileShareDialog(form, listOf(row), { opened += it }) { sent += it }
    }

    private fun click(matcher: SemanticsMatcher): () -> Boolean =
        checkNotNull(compose.awaitNode(matcher).fetchSemanticsNode().config[SemanticsActions.OnClick].action)

    private fun cancel(): () -> Boolean {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return click(hasText(context.getString(R.string.search_cancel)))
    }

    @Test
    fun cancelledTargetCannotSendOrCancelAnEqualReopeningAndCurrentAdmissionIsOnce() {
        show()
        val target = hasTestTag("profile-share-$peer")
        val oldSend = click(target)
        val oldCancel = cancel()
        compose.runOnIdle {
            oldCancel()
            oldSend()
            assertFalse(form.shown)
            assertTrue(sent.isEmpty())
            form.shown = true
        }
        val currentSend = click(target)
        compose.runOnIdle {
            oldCancel()
            oldSend()
            assertTrue(form.shown)
            assertTrue(sent.isEmpty())
            currentSend()
            currentSend()
            assertEquals(listOf(peer), sent)
            assertTrue(form.sending)
        }
        requireCommittedSendCannotRepeat(currentSend)
    }

    private fun requireCommittedSendCannotRepeat(send: () -> Boolean) = compose.runOnIdle {
        form.completeSubmission(Result.success(ChatSubmission.Accepted(byteArrayOf(1))), peer, form.opening)
        send()
        assertEquals(listOf(peer), sent)
    }

    @Test
    fun oldNavigationCancelAndCompletionCannotAlterAnEqualCommittedSuccessor() {
        show()
        val oldOpening = form.opening
        compose.runOnIdle {
            form.completeSubmission(Result.success(ChatSubmission.Accepted(byteArrayOf(1))), peer, form.opening)
        }
        val oldNavigate = click(hasTestTag(PROFILE_SHARE_OPEN_CHAT_TAG))
        val oldCancel = cancel()
        compose.runOnIdle {
            oldCancel()
            form.shown = true
            val queued = ChatSubmission.Queued(byteArrayOf(2), "offline")
            form.completeSubmission(Result.success(queued), peer, form.opening)
        }
        val currentNavigate = click(hasTestTag(PROFILE_SHARE_OPEN_CHAT_TAG))
        compose.runOnIdle {
            oldNavigate()
            oldCancel()
            form.completeSubmission(Result.failure(IllegalStateException("late refusal")), peer, oldOpening)
            assertTrue(form.shown)
            assertTrue(opened.isEmpty())
            assertEquals(R.string.chat_queued, form.status?.text)
            currentNavigate()
            currentNavigate()
            assertEquals(listOf(peer), opened)
            assertFalse(form.shown)
        }
    }

    @Test
    fun disposedDialogRetiresItsCapturedSendAndCompletion() {
        show()
        val send = click(hasTestTag("profile-share-$peer"))
        val opening = form.opening
        compose.runOnIdle { mounted = false }
        compose.onNode(hasTestTag("profile-share-$peer")).assertDoesNotExist()
        compose.runOnIdle {
            send()
            form.completeSubmission(Result.success(ChatSubmission.Accepted(byteArrayOf(1))), peer, opening)
            assertFalse(form.shown)
            assertTrue(sent.isEmpty())
            assertNull(form.sentTo)
        }
    }

    @Test
    fun retiredCompletionCannotUnlockTheCurrentPendingSend() {
        val old = form.opening
        assertTrue(form.admit(old))
        form.shown = false
        form.shown = true
        val current = form.opening
        assertTrue(form.admit(current))
        form.completeSubmission(Result.success(ChatSubmission.Accepted(byteArrayOf(1))), peer, old)
        assertTrue(form.sending)
        assertNull(form.sentTo)
        assertNull(form.status)
        assertFalse(form.admit(old))
        assertFalse(form.admit(current))
        form.completeSubmission(Result.failure(IllegalStateException("current refusal")), peer, current)
        assertFalse(form.sending)
        assertTrue(form.admit(current))
    }
}
