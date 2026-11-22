package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

internal class LinkApprovalUIFixture(private val compose: ComposeContentTestRule) {
    val original = LinkApprovalFixture()
    val successor = LinkApprovalFixture()
    val shown = mutableStateOf(opened())
    val created = AtomicInteger()

    fun show() {
        compose.setContent {
            MaterialTheme {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    LinkApprovalEntry(shown.value) {
                        if (created.getAndIncrement() == 0) original.session else successor.session
                    }
                }
            }
        }
    }

    fun enterCode() {
        compose.awaitNode(hasTestTag("owner-open")).performClick()
        compose.awaitNode(hasTestTag("owner-code-input")).performTextInput("original pasted input")
        compose.onNodeWithTag("owner-code-next").performClick()
        compose.awaitNode(hasTestTag("owner-recipient"))
    }

    fun callback(tag: String): () -> Unit =
        compose.onNodeWithTag(tag).fetchSemanticsNode().config[applicationClickKey]

    fun replaceOwner() {
        compose.runOnIdle { shown.value = opened() }
        compose.waitForIdle()
        compose.awaitNode(hasTestTag("owner-open"))
    }

    fun text(id: Int): String = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun opened(): Opened.Ready {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return Opened.Ready(Slot("owner-slot", File(context.cacheDir, "owner-ui"), SlotState.READY), "original-owner")
    }
}
