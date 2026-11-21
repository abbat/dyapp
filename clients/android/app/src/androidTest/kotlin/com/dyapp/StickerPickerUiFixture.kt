package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatStickerImage
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.SavedStickerPackState
import uniffi.dyapp.StickerPickerEntry
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Actual composer/dialogs with controlled native projections; no transport or guessed transition delay. */
internal class StickerPickerUiFixture(private val compose: ComposeContentTestRule) {
    val owner = mutableStateOf<Any>(Any())
    val scale = mutableStateOf(1f)
    val form = ChatForm(mutableStateOf("draft survives"))
    val messages = listOf(message("me", "original text"))
    val sends = AtomicInteger()
    val sent = AtomicReference<StickerPickerChoice?>()
    val textSends = AtomicInteger()
    val packId = ByteArray(32) { 3 }
    val name = "Wide & tall <pack> ".repeat(2)
    val wide = choice(11u, 96u, 48u)
    val tall = choice(19u, 48u, 96u)
    private val calls = StickerPickerCalls(
        read = { query ->
            when (query) {
                is StickerPickerQuery.Packs -> StickerPickerData.Packs(packs())
                is StickerPickerQuery.Choices -> StickerPickerData.Choices(
                    listOf(if (query.after == 0u) wide else tall),
                    if (query.after == 0u) 37u else null
                )
            }
        },
        send = { choice ->
            sent.set(choice)
            sends.incrementAndGet()
            ChatSubmission.Queued(byteArrayOf(1), "offline")
        }
    )
    private val manager = SavedStickerCalls(
        { packs() },
        { error("unexpected import") },
        { error("unexpected removal") }
    )

    private fun choice(index: UInt, width: UInt, height: UInt) = StickerPickerChoice(
        packId,
        name,
        StickerPickerEntry(index, ":)", width, height, ByteArray((width * height * 4u).toInt()) { -1 })
    )

    private fun packs() = SavedStickerPackPage(
        listOf(SavedStickerPackEntry(packId, 19uL, SavedStickerPackState.Ready(name, 2u))),
        null,
        19uL,
        0uL
    )

    fun show(language: String = "en"): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent { Content(context) }
        return context
    }

    @Composable
    private fun Content(context: Context) {
        LocalizedTestContext(context, scale.value) {
            DYAppTheme {
                SavedStickerManagerHost(owner.value, manager) {
                    val source = ChatStickerSource(owner.value, picker = calls) { _, _ -> ChatStickerImage.Unavailable }
                    val snapshot = ChatThreadSnapshot(
                        ChatHistory(ChatState.OPEN, messages, false),
                        messages.map { ChatItem.Text(it) },
                        stickers = source
                    )
                    val actions = ChatActions({ textSends.incrementAndGet() }, {}, {}, {}, {})
                    MediaChatContent(snapshot, "peer", "me", form, actions)
                }
            }
        }
    }

    fun recent() {
        compose.awaitNode(hasTestTag("chat-stickers") and isEnabled()).performClick()
        compose.awaitNode(hasTestTag("sticker-picker-recent") and isEnabled()).performScrollTo().performClick()
        compose.awaitNode(hasTestTag(choiceTag(wide)) and isEnabled())
    }

    fun choiceTag(choice: StickerPickerChoice) =
        "sticker-picker-choice-" + stickerPackIdText(choice.packId) + "-" + choice.sticker.index

    fun closed() {
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("sticker-picker").fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithTag("chat-input").assertExists()
    }
}
