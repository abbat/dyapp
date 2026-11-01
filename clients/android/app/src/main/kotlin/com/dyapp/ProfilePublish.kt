package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.reflect.KMutableProperty0
import uniffi.dyapp.ProfilePublished
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.ProfileSolve
import uniffi.dyapp.ProfileWorkStep
import uniffi.dyapp.profilePublishBlock

// The localized text for a failed publication, by the stable Rust error text.
internal fun publishError(error: Throwable): Int {
    val message = error.toString()
    return when {
        "blocked: underage" in message -> R.string.profile_publish_error_underage
        "blocked: notAttested" in message -> R.string.profile_publish_error_not_attested
        "blocked: invalidBirthDate" in message -> R.string.profile_publish_error_invalid_birth_date
        "notConnected" in message -> R.string.profile_publish_error_not_connected
        "unsolved" in message -> R.string.profile_publish_error_unsolved
        "readback differs" in message -> R.string.profile_publish_error_readback
        else -> R.string.profile_publish_error_generic
    }
}

private fun blockText(block: String) = when (block) {
    "notAttested" -> R.string.profile_publish_blocked_not_attested
    "invalidBirthDate" -> R.string.profile_publish_blocked_invalid_birth_date
    "underage" -> R.string.profile_publish_blocked_underage
    else -> R.string.profile_eligibility_failed
}

/** The dialog, confirmations and progress of one publication attempt over the saved draft. */
private class PublishState {
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

    fun close() {
        open = false
        adult = false
        this.public = false
        cancelling = false
        failure = null
    }
}

private fun PublishState.eligibility(): Int {
    val checked = block
    val blocked = checked?.getOrNull()
    return when {
        checked == null -> R.string.profile_checking_eligibility
        checked.isFailure -> R.string.profile_eligibility_failed
        blocked != null -> blockText(blocked)
        !adult -> R.string.profile_publish_blocked_not_attested
        !public -> R.string.profile_confirm_public_required
        else -> R.string.profile_eligible
    }
}

// Short saved proof steps until solved, failed (null) or cancelled.
private suspend fun runSteps(solve: ProfileSolve, state: PublishState, attested: Boolean): ProfileWorkStep? {
    var step: ProfileWorkStep?
    do {
        step = withContext(Dispatchers.IO) { runCatching { solve.runStep(attested) } }.getOrNull()
        step?.let { state.nonce = it.nextNonce }
    } while (step != null && !step.solved && !step.cancelled)
    return step
}

// A new handle per attempt resumes the saved nonce; a failure is left in [state].
private suspend fun solveProof(opened: Opened.Ready, state: PublishState, attested: Boolean): Boolean {
    val solve = withContext(Dispatchers.IO) {
        runCatching { RustBridge.withKey(opened) { dir, key -> ProfileSolve(dir, key) } }
    }.getOrNull()
    state.solve = solve
    val step = try {
        solve?.let { runSteps(it, state, attested) }
    } finally {
        state.solve = null
        solve?.close()
    }
    val failure = when {
        step == null -> R.string.profile_proof_failed
        !step.solved -> R.string.profile_proof_cancelled
        else -> null
    }
    failure?.let { state.failure = it to R.string.profile_retry_proof }
    return failure == null
}

// Proves and publishes the saved draft; each attempt clears both confirmations, and success closes the dialog.
private suspend fun prepareAndPublish(opened: Opened.Ready, state: PublishState, visibility: VisibilityState) {
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
            runCatching { RustBridge.withChats(opened) { it.publishProfile(attested) } }
        }
        result.fold({
            state.published = it
            state.open = false
            visibility.shown(it.version)
        }, { state.failure = publishError(it) to R.string.profile_retry_publish })
    } finally {
        state.publishing = false
        state.running = false
        state.cancelling = false
    }
}

@Composable
private fun Confirmation(checked: KMutableProperty0<Boolean>, text: Int, tag: String) {
    Row(
        Modifier.fillMaxWidth().toggleable(checked.get(), role = Role.Checkbox, onValueChange = checked::set)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Checkbox(checked.get(), onCheckedChange = null)
        Text(stringResource(text))
    }
}

/**
 * The form's save and publish actions, and the dialog with the saved public fields, fresh 18+ and
 * public-data confirmations, the shared eligibility check and the cancellable proof and publication.
 * The signed readback stays on the form. The caller recreates this section on every edit or save,
 * which clears the confirmations and cancels a running attempt.
 */
@Composable
@Suppress("LongParameterList") // The form's state and its save button, all from one caller.
internal fun PublishSection(
    opened: Opened.Ready,
    scales: List<ProfileScale>,
    visibility: VisibilityState,
    unsaved: Boolean,
    save: @Composable RowScope.() -> Unit,
) {
    val state = remember { PublishState() }
    val scope = rememberCoroutineScope()
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
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        save()
        Button(onClick = { state.open = true }, Modifier.weight(1f).testTag("profile-publish"), enabled = !unsaved) {
            Text(stringResource(R.string.profile_publish))
        }
    }
    if (unsaved) {
        Text(stringResource(R.string.profile_save_changes_before_publish),
            Modifier.testTag("profile-publish-eligibility"))
    }
    if (state.open) {
        PublishDialog(state, scales, { loadDraft(opened, scales) }) {
            scope.launch { prepareAndPublish(opened, state, visibility) }
        }
    }
    state.published?.let { published ->
        Text(stringResource(R.string.profile_published_version, "${published.version}"),
            Modifier.testTag("profile-published-readback"))
        ProfilePreview(published.draft.draft(), scales, LocalContext.current, R.string.profile_published)
    }
}

// Reads the saved draft once per opening; a running attempt keeps the dialog up until it ends or fails.
@Composable
private fun PublishDialog(
    state: PublishState,
    scales: List<ProfileScale>,
    read: () -> ProfileDraft,
    publish: () -> Unit,
) {
    val draft = remember { runCatching(read) }
    val eligibility = state.eligibility()
    AlertDialog(
        onDismissRequest = { if (!state.running) state.close() },
        title = { Text(stringResource(R.string.publish_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(Modifier.testTag("profile-public-fields")) {
                    draft.fold({ ProfilePreview(it, scales, LocalContext.current, R.string.publish_intro) },
                        { Text(stringResource(R.string.profile_read_failed)) })
                }
                if (state.running) {
                    PublishProgress(state)
                } else {
                    Confirmation(state::adult, R.string.profile_confirm_adult, "profile-attested")
                    Confirmation(state::public, R.string.profile_confirm_public, "profile-disclosure")
                    Text(stringResource(eligibility), Modifier.testTag("profile-publish-eligibility"))
                }
                state.failure?.let { (text, _) ->
                    Text(stringResource(text), Modifier.testTag("profile-publish-error")
                        .semantics { liveRegion = LiveRegionMode.Polite })
                }
            }
        },
        confirmButton = {
            TextButton(onClick = publish, Modifier.testTag("profile-publish-confirm"),
                enabled = draft.isSuccess && !state.running && eligibility == R.string.profile_eligible) {
                Text(stringResource(state.failure?.second ?: R.string.publish_confirm))
            }
        },
        dismissButton = {
            // Only the proof can stop; the publication itself runs to its result.
            TextButton(onClick = {
                if (state.running) {
                    state.cancelling = true
                    state.solve?.cancel()
                } else {
                    state.close()
                }
            }, Modifier.testTag("profile-cancel"),
                enabled = !state.cancelling && (!state.running || state.solve != null)) {
                Text(stringResource(R.string.remove_cancel))
            }
        },
    )
}

@Composable
private fun PublishProgress(state: PublishState) {
    Text(stringResource(R.string.publish_working), Modifier.semantics { liveRegion = LiveRegionMode.Polite })
    LinearProgressIndicator(Modifier.fillMaxWidth())
    if (state.nonce > 0uL) {
        Text(stringResource(R.string.profile_proof_nonce, "${state.nonce}"),
            Modifier.testTag("profile-proof-progress"))
    }
    if (state.publishing) {
        Text(stringResource(R.string.profile_publishing), Modifier.testTag("profile-publish-status"))
    }
}
