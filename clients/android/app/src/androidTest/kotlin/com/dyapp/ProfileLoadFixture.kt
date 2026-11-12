package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile
import java.io.File

internal abstract class ProfileLoadFixture : ProfileViewFixture() {
    protected var latest: State<ProfileLoad>? = null

    protected class Selection(profile: FoundProfile, feed: ChatFeed) {
        var peer by mutableStateOf(profile.peerId)
        var known by mutableStateOf<FoundProfile?>(profile)
        var owner by mutableStateOf(feed)
        var attempt by mutableIntStateOf(0)
    }

    protected class FetchGate {
        val entered = CompletableDeferred<Unit>()
        val released = CompletableDeferred<FoundProfile?>()
        val finished = CompletableDeferred<Unit>()

        suspend fun fetch(): FoundProfile? {
            currentCoroutineContext().job.invokeOnCompletion { finished.complete(Unit) }
            entered.complete(Unit)
            // Deliberately return after cancellation to challenge the producer's ownership guard.
            return withContext(NonCancellable) { released.await() }
        }
    }

    protected fun owner(id: String): ChatFeed {
        // Only an ownership token: the controlled fetch never opens native storage.
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return ChatFeed(Opened.Ready(Slot(id, File(context.cacheDir, id), SlotState.READY), id))
    }

    protected fun selection() = Selection(found, owner("profile-load-owner"))

    protected fun showLoad(selection: Selection, fetch: suspend () -> FoundProfile?) = compose.setContent {
        val state = selection.owner.rememberProfileResponse(
            selection.peer,
            selection.known,
            selection.attempt,
            fetch,
        )
        latest = state
        Text(
            when (val value = state.value) {
                is ProfileLoad.Shown -> value.found.peerId
                ProfileLoad.Loading -> "loading"
                ProfileLoad.Missing -> "missing"
                is ProfileLoad.Failed -> "failed"
            }
        )
    }

    // A fresh failure deadline for each actual transition; waiting through compose lets the
    // test dispatcher run the effects that complete the step.
    protected fun awaitStep(signal: Deferred<Unit>) = compose.waitUntil(CHECK_WAIT_MS) { signal.isCompleted }
}
