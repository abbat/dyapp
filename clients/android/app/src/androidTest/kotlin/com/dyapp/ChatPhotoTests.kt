package com.dyapp

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatPhoto
import java.util.concurrent.atomic.AtomicInteger

class ChatPhotoTests {
    @get:Rule
    val compose = createComposeRule()
    private val calls = AtomicInteger()
    private val photo = ChatPhoto(
        groupId = byteArrayOf(1), author = "peer", id = ByteArray(16),
        hlc = 1uL, sentAtMs = 0uL, size = 12uL, mime = "image/jpeg",
        preview = byteArrayOf(), width = 3u, height = 2u,
        unread = true, forwarded = false, delivery = null,
    )

    private fun string(id: Int) =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun show(stranger: Boolean, automatic: Boolean) = compose.setContent {
        CompositionLocalProvider(
            LocalChatPhotoAuto provides { automatic },
            LocalChatPhotoLoader provides {
                calls.incrementAndGet()
                error("notFound")
            },
        ) { ChatPhotoBubble(photo, stranger) }
    }

    @Test
    fun strangerRequiresRevealThenAnExplicitDownloadEvenWhenAutomaticIsAllowed() {
        show(stranger = true, automatic = true)
        compose.onNodeWithText(string(R.string.chat_photo_show)).assertExists()
        compose.onNodeWithText(string(R.string.chat_photo_download)).assertDoesNotExist()
        compose.runOnIdle { assertEquals(0, calls.get()) }
        compose.onNodeWithText(string(R.string.chat_photo_show)).performClick()
        compose.runOnIdle { assertEquals(0, calls.get()) }
        compose.onNodeWithText(string(R.string.chat_photo_download)).performClick()
        compose.waitUntil(CHECK_WAIT_MS) { calls.get() == 1 }
        compose.waitUntil(CHECK_WAIT_MS) {
            compose.shown(
                androidx.compose.ui.test.hasText(string(R.string.chat_photo_gone))
            )
        }
    }

    @Test
    fun layoutBoundsAndIdentityKeepUntrustedPhotosDistinct() {
        assertEquals(20f, photoAspect(photo.copy(width = UInt.MAX_VALUE, height = 1u)), 0f)
        assertEquals(0.05f, photoAspect(photo.copy(width = 1u, height = UInt.MAX_VALUE)), 0f)
        org.junit.Assert.assertNotEquals(photoKey(photo), photoKey(photo.copy(author = "other")))
        org.junit.Assert.assertNotEquals(photoKey(photo), photoKey(photo.copy(groupId = byteArrayOf(2))))
    }

    @Test
    fun acceptedPhotoUsesTheCoreAutomaticDecision() {
        show(stranger = false, automatic = true)
        compose.waitUntil(CHECK_WAIT_MS) { calls.get() == 1 }
    }

    @Test
    fun deniedAutomaticDecisionLeavesDownloadForATap() {
        show(stranger = false, automatic = false)
        compose.onNodeWithText(string(R.string.chat_photo_download)).assertExists()
        compose.runOnIdle { assertEquals(0, calls.get()) }
    }
}
