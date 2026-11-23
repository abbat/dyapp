package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.SearchRecoverySaved
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

internal class SearchRecoveryUIOwner(confirm: () -> SearchRecoverySaved) {
    val confirmed = AtomicInteger()
    val closed = AtomicInteger()
    val visibility = VisibilityState(1uL, false)
    val editor = ProfileEditor(ProfileDraft(name = "Saved"), "en")
    val session = SearchRecoverySession {
        SearchRecoveryCalls({
            confirmed.incrementAndGet()
            confirm()
        }, { error("Unexpected UI delivery retry") }, { closed.incrementAndGet() })
    }
    val opened = Opened.Ready(
        Slot(
            "same-slot",
            File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "recovery-ui"),
            SlotState.READY
        ),
        "same-peer"
    )

    fun close() = runBlocking { withTimeout(5_000) { session.retire().await().getOrThrow() } }
}

/** Equal public coordinates are separate openings, mirroring ProfileDraftForm's key(opened). */
internal class SearchRecoveryUIFixture(
    private val compose: ComposeContentTestRule,
    val original: SearchRecoveryUIOwner,
    val successor: SearchRecoveryUIOwner
) {
    private val current = mutableStateOf(original)

    fun show() {
        compose.setContent {
            MaterialTheme {
                current.value.let { owner ->
                    key(owner.opened) {
                        Column(Modifier.verticalScroll(rememberScrollState())) {
                            ProfileNameField(owner.editor.name)
                            SearchRecoveryCard(owner.opened, owner.visibility) { owner.session }
                        }
                    }
                }
            }
        }
        ready(original)
    }

    fun replaceOwner() {
        compose.runOnIdle { current.value = successor }
        ready(successor)
    }

    private fun ready(owner: SearchRecoveryUIOwner) {
        compose.awaitSnapshot(owner.session.state, 5_000, "original-owner search review") {
            !it.busy && it.opening != null
        }
        compose.awaitNode(hasTestTag("search-recovery-action"))
    }

    fun callback(tag: String): () -> Boolean =
        checkNotNull(compose.onNodeWithTag(tag).fetchSemanticsNode().config[SemanticsActions.OnClick].action)
}
