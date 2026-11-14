package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private const val PHOTO_SEND_STEP_SECONDS = 30L

/** Controls native-sender completion without an emulator/node latency assumption. */
private class PhotoConfirmationSend {
    val calls = AtomicInteger()
    val started = CompletableFuture<Unit>()
    val release = CompletableFuture<Unit>()

    fun submit(photo: PreparedPhoto): Result<ChatSubmission> {
        check(photo.full.isNotEmpty())
        if (calls.incrementAndGet() == 1) {
            started.complete(Unit)
            release.get(PHOTO_SEND_STEP_SECONDS, TimeUnit.SECONDS)
            return Result.failure(IllegalStateException("full"))
        }
        return Result.success(ChatSubmission.Queued(byteArrayOf(1), "offline"))
    }
}

class ChatPhotoConfirmationTests {
    @get:Rule
    val compose = createComposeRule()
    private val photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2), 1u, 1u)
    private val draft = ChatPhotoDraft().apply { photo = this@ChatPhotoConfirmationTests.photo }
    private val sender = PhotoConfirmationSend()

    private fun text(id: Int) =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun show() = compose.setContent {
        ChatPhotoDraftDialog(draft, sender::submit, enabled = true)
    }

    @Test
    fun cancelDropsOnlyThePreparedDraftWithoutSending() {
        show()
        val cancel = callback(R.string.search_cancel)
        val send = callback(R.string.chat_send)
        compose.runOnIdle {
            cancel()
            send()
            assertNull(draft.photo)
            assertFalse(draft.busy)
            assertEquals(0, sender.calls.get())
        }
    }

    private fun callback(id: Int) = checkNotNull(
        compose.onNodeWithText(text(id))
            .fetchSemanticsNode()
            .config[SemanticsActions.OnClick].action
    )

    private fun confirmTwiceBeforeRecomposition() {
        val send = callback(R.string.chat_send)
        val cancel = callback(R.string.search_cancel)
        compose.runOnIdle {
            send()
            send()
            cancel()
            assertTrue(draft.busy)
            assertSame(photo, draft.photo)
        }
    }

    @Test
    fun duplicateConfirmationAndRefusalKeepOneRetryableIntent() {
        show()
        try {
            confirmTwiceBeforeRecomposition()
            sender.started.get(PHOTO_SEND_STEP_SECONDS, TimeUnit.SECONDS)
            compose.onNodeWithText(text(R.string.chat_send)).assertIsNotEnabled()
            compose.onNodeWithText(text(R.string.search_cancel)).assertIsNotEnabled()
            assertEquals(1, sender.calls.get())
        } finally {
            sender.release.complete(Unit)
        }
        compose.awaitNode(hasText(text(R.string.chat_photo_full)))
        compose.runOnIdle {
            assertSame(photo, draft.photo)
            assertFalse(draft.busy)
        }
        compose.onNodeWithText(text(R.string.chat_send)).assertIsEnabled().performClick()
        compose.awaitNode(hasText(text(R.string.chat_queued)))
        compose.runOnIdle {
            assertEquals(2, sender.calls.get())
            assertNull(draft.photo)
            assertFalse(draft.busy)
        }
    }
}
