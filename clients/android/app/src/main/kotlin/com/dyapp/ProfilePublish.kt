package com.dyapp

import android.util.Log
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ProfilePublished
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.ProfileSolve
import uniffi.dyapp.ProfileWorkStep
import uniffi.dyapp.profilePublishBlock
import uniffi.dyapp.profileScales

internal const val ELIGIBILITY_TAG = "profile-publish-eligibility"

// The localized text for a failed publication, by the stable Rust error text, which goes to the log.
internal fun publishError(error: Throwable): Int {
    val message = error.toString()
    Log.w("dyapp", "Publication failed: $message")
    return when {
        "blocked: underage" in message -> R.string.profile_publish_error_underage
        "blocked: notAttested" in message -> R.string.profile_publish_error_not_attested
        "blocked: invalidBirthDate" in message -> R.string.profile_publish_error_invalid_birth_date
        "notConnected" in message -> R.string.profile_publish_error_not_connected
        "unsolved" in message -> R.string.profile_publish_error_unsolved
        "readback differs" in message -> R.string.profile_publish_error_readback
        error.message == "full" -> R.string.profile_publish_error_full
        else -> R.string.profile_publish_error_generic
    }
}

/** The dialog, confirmations and progress of one publication attempt over the saved draft. */
internal class PublishState(val visibility: VisibilityState, private val onPublished: (ProfilePublished) -> Unit) {
    var open by mutableStateOf(false)
    var adult by mutableStateOf(false)
    var public by mutableStateOf(false)

    /** The `profile_publish_block` result for [adult]; null while it is checked. */
    var block by mutableStateOf<Result<String?>?>(null)
    var running by mutableStateOf(false)
    var solve by mutableStateOf<ProfileSolve?>(null)
    var cancelling by mutableStateOf(false)
    var publishing by mutableStateOf(false)
    var nonce by mutableStateOf(0uL)

    /** The proof or publication failure shown, and the retry label it offers. */
    var failure by mutableStateOf<Pair<Int, Int>?>(null)
    var published by mutableStateOf<ProfilePublished?>(null)

    fun completed(result: ProfilePublished) {
        published = result
        open = false
        visibility.observed(result.currentVersion, result.hidden)
        onPublished(result)
    }

    fun close() {
        open = false
        adult = false
        this.public = false
        cancelling = false
        failure = null
    }
}

// Short saved proof steps until solved, failed (null) or cancelled.
private suspend fun runSteps(solve: ProfileSolve, state: PublishState, attested: Boolean): ProfileWorkStep? {
    var step: ProfileWorkStep?
    do {
        step = withContext(Dispatchers.IO) {
            runCatching { synchronized(RustBridge.draftWrites) { solve.runStep(attested) } }
        }.getOrNull()
        step?.let { state.nonce = it.nextNonce }
    } while (step != null && !step.solved && !step.cancelled)
    return step
}

// A new handle per attempt resumes the saved nonce; a failure is left in [state].
private suspend fun solveProof(opened: Opened.Ready, state: PublishState, attested: Boolean): Boolean {
    val step = runCatching {
        withNativeResource(opened::profileSolver) { solve ->
            state.solve = solve
            try {
                runSteps(solve, state, attested)
            } finally {
                state.solve = null
            }
        }
    }.getOrNull()
    currentCoroutineContext().ensureActive()
    val failure = when {
        step == null -> R.string.profile_proof_failed
        !step.solved -> R.string.profile_proof_cancelled
        else -> null
    }
    failure?.let { state.failure = it to R.string.profile_retry_proof }
    return failure == null
}

// Proves and publishes the saved draft; each attempt clears both confirmations, and success closes the dialog.
private suspend fun prepareAndPublish(
    opened: Opened.Ready,
    state: PublishState,
) {
    val attested = state.adult
    state.adult = false
    state.public = false
    state.cancelling = false
    state.failure = null
    state.published = null
    state.running = true
    try {
        if (!solveProof(opened, state, attested)) return
        state.publishing = true
        val result = withContext(Dispatchers.IO) {
            runCatching {
                synchronized(RustBridge.draftWrites) { RustBridge.withChats(opened) { it.publishProfile(attested) } }
            }
        }
        result.fold(state::completed) { state.failure = publishError(it) to R.string.profile_retry_publish }
    } finally {
        state.publishing = false
        state.running = false
        state.cancelling = false
    }
}

/**
 * The form's save and publish actions, and the dialog with the saved public fields, fresh 18+ and
 * public-data confirmations, the shared eligibility check and the cancellable proof and publication.
 * The signed readback stays on the form. The caller recreates this section on every edit or save,
 * which clears the confirmations and cancels a running attempt.
 */
@Composable
internal fun PublishSection(
    opened: Opened.Ready,
    state: PublishState,
    unsaved: Boolean,
    save: @Composable RowScope.() -> Unit,
) {
    val scales = remember { profileScales() }
    val scope = rememberCoroutineScope()
    PublishEffects(opened, state, state.visibility, unsaved)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        save()
        Button(onClick = { state.open = true }, Modifier.weight(1f).testTag("profile-publish"), enabled = !unsaved) {
            Text(stringResource(R.string.profile_publish))
        }
    }
    if (unsaved) {
        Text(
            stringResource(R.string.profile_save_changes_before_publish),
            Modifier.testTag(ELIGIBILITY_TAG)
        )
    }
    if (state.open) {
        PublishDialog(state, scales, { loadDraft(opened) }) {
            scope.launch { prepareAndPublish(opened, state) }
        }
    }
    state.published?.let { PublishedReadback(it, scales) }
}

@Composable
private fun PublishedReadback(published: ProfilePublished, scales: List<ProfileScale>) {
    Text(
        stringResource(R.string.profile_published_version, "${published.version}"),
        Modifier.testTag("profile-published-readback")
    )
    ProfilePreview(published.draft.draft(), scales, R.string.profile_published)
}

@Composable
private fun PublishEffects(
    opened: Opened.Ready,
    state: PublishState,
    visibility: VisibilityState,
    unsaved: Boolean,
) {
    LaunchedEffect(visibility.publishAsked) {
        if (visibility.publishAsked) {
            visibility.publishAsked = false
            state.open = true
        }
    }
    LaunchedEffect(state.adult, unsaved) {
        if (unsaved) return@LaunchedEffect
        val attested = state.adult
        state.block = null
        state.block = withContext(Dispatchers.IO) {
            runCatching { RustBridge.withKey(opened) { dir, key -> profilePublishBlock(dir, key, attested) } }
        }
    }
    DisposableEffect(Unit) { onDispose { state.solve?.cancel() } }
}
