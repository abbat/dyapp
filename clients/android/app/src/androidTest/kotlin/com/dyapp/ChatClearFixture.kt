package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.ChatClearTarget
import uniffi.dyapp.ContactNameView
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

internal class ChatClearFixture(private val compose: ComposeContentTestRule) {
    val peer = "0123456789abcdef".repeat(4)
    val form = ChatForm(mutableStateOf("")).apply { dialog.value = ChatDialog.CLEAR }
    val scale = mutableFloatStateOf(1f)
    val fail = AtomicBoolean(false)
    val clears = AtomicInteger(0)
    val calls = mutableStateOf(reader())

    fun reader() = ChatClearCalls {
        check(!fail.get()) { "controlled target read failure" }
        ChatClearTarget("owner", peer)
    }

    fun show(language: String = "en"): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent {
            LocalizedTestContext(context, scale.floatValue) {
                MaterialTheme {
                    CompositionLocalProvider(LocalChatClearCalls provides calls.value) {
                        if (form.dialog.value == ChatDialog.CLEAR) {
                            ChatClearDialog(form, ContactNameView(peer, "Robin", null, byteArrayOf())) {
                                clears.incrementAndGet()
                            }
                        }
                    }
                }
            }
        }
        return context
    }

    fun ready() = compose.awaitNode(hasTestTag("chat-clear-confirm") and isEnabled())
}
