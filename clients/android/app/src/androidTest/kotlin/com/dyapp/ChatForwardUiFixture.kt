package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission
import java.io.File
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal class ChatForwardUiFixture(private val compose: ComposeContentTestRule) {
    val scale = mutableFloatStateOf(1f)
    val me = "b".repeat(64)
    val peer = "a".repeat(64)
    val opened = mutableStateOf(Opened.Ready(Slot("slot", File("build/unopened-forward-ui"), SlotState.READY), me))
    val form = ChatForm(mutableStateOf("draft")).apply {
        held.value = "held"
        editing.value = "edit key"
    }
    val own = message(me, "Own text")
    val received = message(peer, "Received text").copy(id = ByteArray(16) { 2 })
    val choices = (1..40).map { it.toString(16).padStart(64, '0') }
    val offered = AtomicReference(choices)
    val failRead = AtomicBoolean()
    val sends = AtomicInteger()
    val sent = AtomicReference<Pair<ChatForwardSource, String>>()
    val navigated = CopyOnWriteArrayList<String>()
    val current = AtomicReference<ChatForwardContext>()
    var outcome: ChatSubmission = ChatSubmission.Accepted(ByteArray(16))
    var failSend = false
    val calls = ChatForwardCalls(
        {
            check(!failRead.get()) { "controlled read failure" }
            offered.get()
        },
        { source, target ->
            sends.incrementAndGet()
            sent.set(source to target)
            check(!failSend) { "controlled precommit refusal" }
            outcome
        }
    )

    fun show(language: String = "en"): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val localized = base.createConfigurationContext(configuration)
        compose.setContent { LocalizedTestContext(localized, scale.floatValue) { Render() } }
        return localized
    }

    @Composable
    private fun Render() {
        val context = remember(opened.value) { ChatForwardContext(me, peer, ChatForwardPresentation(), calls) }
        SideEffect { current.set(context) }
        MaterialTheme {
            ChatForwardBoundary(context, { navigated.add(it) }) {
                ChatContent(
                    peer,
                    me,
                    ChatHistory(ChatState.OPEN, listOf(own, received), false),
                    form,
                    ChatActions({}, {}, {}, {}, {})
                )
            }
        }
    }

    fun ready() = compose.awaitNode(hasTestTag("chat-forward-confirm") and isEnabled())
}
