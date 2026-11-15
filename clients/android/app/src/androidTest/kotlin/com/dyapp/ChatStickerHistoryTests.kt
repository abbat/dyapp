package com.dyapp

import android.content.res.Configuration
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToNode
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSticker
import java.util.Locale

/** Existing metadata fallback in the actual mixed-history renderer; no media or native mock. */
class ChatStickerHistoryTests {
    @get:Rule
    val compose = createComposeRule()

    private fun sticker(author: String, packName: String, emoji: String) = ChatSticker(
        groupId = byteArrayOf(1), author = author, id = byteArrayOf(if (author == "me") 1 else 2),
        hlc = 1uL, sentAtMs = 0uL, packId = ByteArray(32), sha256 = ByteArray(32),
        emoji = emoji, packName = packName, unread = false, delivery = null,
    )

    private fun show(language: String, packName: String): List<String> {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(
            base.resources.configuration
        ).apply { setLocale(Locale.forLanguageTag(language)) }
        val localized = base.createConfigurationContext(configuration)
        val texts = listOf(message("peer", "before sticker"), message("me", "after sticker"))
        val stickers = listOf(sticker("peer", packName, "\uD83D\uDE3A"), sticker("me", packName, "\uD83E\uDD8A"))
        val items = listOf(
            ChatItem.Text(texts[0]),
            ChatItem.Sticker(stickers[0]),
            ChatItem.Text(texts[1]),
            ChatItem.Sticker(stickers[1])
        )
        val snapshot = ChatThreadSnapshot(ChatHistory(ChatState.OPEN, texts, false), items)
        val form = ChatForm(mutableStateOf(""))
        compose.setContent {
            val registry = checkNotNull(LocalActivityResultRegistryOwner.current)
            CompositionLocalProvider(
                LocalContext provides localized,
                LocalConfiguration provides configuration,
                LocalActivityResultRegistryOwner provides registry,
            ) {
                MediaChatContent(snapshot, "peer", "me", form, ChatActions({}, {}, {}, {}, {}))
            }
        }
        return texts.map { checkNotNull(it.text) } +
            stickers.map { localized.getString(R.string.chat_sticker_fallback, it.emoji, it.packName) }
    }

    private fun verify(language: String, packName: String) {
        val labels = show(language, packName)
        labels.forEach { label ->
            compose.onNodeWithTag("chat-thread").performScrollToNode(hasText(label))
            compose.onNodeWithText(label).assertIsDisplayed()
        }
    }

    @Test
    fun englishMixedHistoryKeepsBothTextsAndOwnAndPeerStickerFallbacks() = verify("en", "Cats")

    @Test
    fun russianMixedHistoryKeepsBothTextsAndOwnAndPeerStickerFallbacks() = verify("ru", "Cats")

    @Test
    fun englishPackLabelsRemainLiteralWhenWrapping() = verify("en", "<cats & friends> \"pack\" ".repeat(2))

    @Test
    fun russianPackLabelsRemainLiteralWhenWrapping() = verify("ru", "<cats & friends> \"pack\" ".repeat(2))
}
