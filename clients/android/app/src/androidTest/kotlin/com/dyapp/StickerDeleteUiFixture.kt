package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSticker
import uniffi.dyapp.ChatStickerImage
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal class StickerDeleteUiFixture(private val compose: ComposeContentTestRule) {
    val owner = mutableStateOf<Any>(Any())
    val form = ChatForm(mutableStateOf("keep the draft"))
    val deletes = AtomicInteger()
    val selected = AtomicReference<ChatSticker?>()
    val own = ChatSticker(
        byteArrayOf(1), "me", ByteArray(16) { 7 }, 1uL, 1uL, ByteArray(32), ByteArray(32),
        ":)", "Pack <not markup>", false, null
    )
    private val foreign = own.copy(author = "peer")

    fun show() = compose.setContent {
        DYAppTheme {
            val source = ChatStickerSource(owner.value, delete = {
                deletes.incrementAndGet()
                selected.set(it)
                ChatSubmission.Queued(byteArrayOf(1), "offline")
            }) { _, _ -> ChatStickerImage.Unavailable }
            val snapshot = ChatThreadSnapshot(
                ChatHistory(ChatState.OPEN, emptyList(), false),
                listOf(ChatItem.Sticker(own), ChatItem.Sticker(foreign)),
                stickers = source
            )
            MediaChatContent(snapshot, "peer", "me", form, ChatActions({}, {}, {}, {}, {}))
        }
    }

    fun ask() {
        compose.onNodeWithTag("own-sticker-menu-target").performSemanticsAction(SemanticsActions.OnLongClick) { it() }
        compose.awaitNode(hasTestTag("sticker-message-delete") and isEnabled()).performClick()
        compose.awaitNode(hasTestTag("sticker-delete-confirm") and isEnabled())
    }

    fun closed() {
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("sticker-delete-dialog").fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithTag("chat-input").assertExists()
    }
}
