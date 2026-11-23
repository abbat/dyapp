package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.unit.dp
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.SearchRecoveryException
import uniffi.dyapp.SearchRecoverySaved
import java.io.File
import java.util.concurrent.CountDownLatch

internal enum class SearchRecoveryVisualState(
    val screen: String,
    val message: Int,
    val preparationFailure: SearchRecoveryException? = null
) {
    REVIEW("search-recovery-review", R.string.search_recovery_review),
    BUSY("search-recovery-busy", R.string.search_recovery_busy),
    PENDING("search-recovery-pending", R.string.search_recovery_pending),
    SENT("search-recovery-sent", R.string.search_recovery_sent),
    READ_ERROR(
        "search-recovery-read-error",
        R.string.search_recovery_error,
        SearchRecoveryException.Failed("Controlled read failure")
    ),
    CONFIRM_ERROR("search-recovery-confirm-error", R.string.search_recovery_error),
    RETRY_ERROR("search-recovery-retry-error", R.string.search_recovery_error),
    STALE("search-recovery-stale", R.string.search_recovery_stale, SearchRecoveryException.Changed()),
    REVIEW_REQUIRED(
        "search-recovery-review-required",
        R.string.search_recovery_review_required,
        SearchRecoveryException.ReviewRequired()
    ),
    LINK_PENDING(
        "search-recovery-link-pending",
        R.string.search_recovery_link_pending,
        SearchRecoveryException.LinkPending()
    ),
    VERSION_EXHAUSTED(
        "search-recovery-version-exhausted",
        R.string.search_recovery_version_exhausted,
        SearchRecoveryException.VersionExhausted()
    ),
    WRONG_OWNER(
        "search-recovery-wrong-owner",
        R.string.search_recovery_wrong_owner,
        SearchRecoveryException.WrongOwner()
    )
}

internal class SearchRecoveryVisualScene(
    val state: SearchRecoveryVisualState,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float
) {
    val metadata = NativeScreenVisualScene("search-recovery-root", state.screen, context, dark, fontScale)
    val entered = CompletableDeferred<Unit>()
    private val release = CountDownLatch(1)
    val visibility = VisibilityState(1uL, false)
    val opened = Opened.Ready(
        Slot("visual-only", File(context.cacheDir, "search-recovery-visual"), SlotState.READY),
        PRESENCE_VISUAL_PEER
    )
    val session = SearchRecoverySession {
        state.preparationFailure?.let { throw it }
        SearchRecoveryCalls(::confirm, ::retry, {})
    }

    private fun confirm(): SearchRecoverySaved {
        if (state == SearchRecoveryVisualState.BUSY) {
            entered.complete(Unit)
            release.await()
        }
        check(state != SearchRecoveryVisualState.CONFIRM_ERROR) { "Controlled confirmation refusal" }
        return SearchRecoverySaved(2uL, true, state != SearchRecoveryVisualState.SENT)
    }

    private fun retry(): SearchRecoverySaved {
        if (state == SearchRecoveryVisualState.RETRY_ERROR) {
            throw SearchRecoveryException.Failed("Controlled delivery failure")
        }
        return SearchRecoverySaved(2uL, true, true)
    }

    fun close() {
        release.countDown()
        runBlocking { withTimeout(5_000) { session.retire().await().getOrThrow() } }
    }
}

/** Production card and theme; fixtures have no registry, credentials, signing or network access. */
internal class SearchRecoveryVisualHost(private val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<SearchRecoveryVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: SearchRecoveryVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: SearchRecoveryVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                Surface(Modifier.fillMaxSize().testTag("search-recovery-root")) {
                    Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp)) {
                        SearchRecoveryCard(scene.opened, scene.visibility) { scene.session }
                    }
                }
            }
        }
    }
}
