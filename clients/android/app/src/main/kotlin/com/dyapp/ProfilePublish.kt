package com.dyapp

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
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
import androidx.compose.ui.semantics.Role
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.reflect.KMutableProperty0
import uniffi.dyapp.ProfilePublished
import uniffi.dyapp.ProfilePublisher
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.ProfileSolve
import uniffi.dyapp.ProfileWorkStep
import uniffi.dyapp.profilePublishBlock

// The localized text for a failed publication, by the stable Rust error text.
private fun publishError(error: Throwable): Int {
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

/** Confirmations and progress of one publication attempt over the saved draft. */
private class PublishState {
    var adult by mutableStateOf(false)
    var public by mutableStateOf(false)

    /** The `profile_publish_block` result for [adult]; null while it is checked. */
    var block by mutableStateOf<Result<String?>?>(null)
    var solve by mutableStateOf<ProfileSolve?>(null)
    var cancelling by mutableStateOf(false)
    var publishing by mutableStateOf(false)
    var nonce by mutableStateOf(0uL)

    /** The proof or publication failure shown, and the retry label it offers. */
    var failure by mutableStateOf<Pair<Int, Int>?>(null)
    var published by mutableStateOf<ProfilePublished?>(null)

    val busy get() = solve != null || publishing
}

private fun PublishState.eligibility(unsaved: Boolean): Int {
    val checked = block
    val blocked = checked?.getOrNull()
    return when {
        unsaved -> R.string.profile_save_changes_before_publish
        busy -> R.string.profile_proof_working
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

// Proves and publishes the saved draft; each attempt clears both confirmations.
private suspend fun prepareAndPublish(opened: Opened.Ready, state: PublishState) {
    val attested = state.adult
    state.adult = false
    state.public = false
    state.cancelling = false
    state.failure = null
    state.published = null
    if (!solveProof(opened, state, attested)) return
    state.publishing = true
    val result = withContext(Dispatchers.IO) {
        runCatching {
            RustBridge.withKey(opened) { dir, key -> ProfilePublisher(dir).use { it.publish(key, attested) } }
        }
    }
    state.publishing = false
    result.fold({ state.published = it }, { state.failure = publishError(it) to R.string.profile_retry_publish })
}

@Composable
private fun Confirmation(checked: KMutableProperty0<Boolean>, enabled: Boolean, text: Int, tag: String) {
    Row(
        Modifier.toggleable(checked.get(), enabled = enabled, role = Role.Checkbox, onValueChange = checked::set)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Checkbox(checked.get(), onCheckedChange = null, enabled = enabled)
        Text(stringResource(text))
    }
}

/**
 * Fresh 18+ and public-data confirmations, the shared eligibility check, the cancellable proof and
 * publication with its signed readback. The caller recreates it on every edit or save, which clears
 * the confirmations and cancels a running attempt.
 */
@Composable
internal fun PublishSection(opened: Opened.Ready, scales: List<ProfileScale>, unsaved: Boolean) {
    val state = remember { PublishState() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(state.adult, unsaved) {
        if (unsaved) return@LaunchedEffect
        val attested = state.adult
        state.block = null
        state.block = withContext(Dispatchers.IO) {
            runCatching { RustBridge.withKey(opened) { dir, key -> profilePublishBlock(dir, key, attested) } }
        }
    }
    DisposableEffect(Unit) { onDispose { state.solve?.cancel() } }
    val editable = !unsaved && !state.busy
    Confirmation(state::adult, editable, R.string.profile_confirm_adult, "profile-confirm-adult")
    Confirmation(state::public, editable, R.string.profile_confirm_public, "profile-confirm-public")
    val eligibility = state.eligibility(unsaved)
    Text(stringResource(eligibility), Modifier.testTag("profile-publish-eligibility"))
    Button(onClick = { scope.launch { prepareAndPublish(opened, state) } },
        Modifier.testTag("profile-prepare-publish"), enabled = eligibility == R.string.profile_eligible) {
        Text(stringResource(state.failure?.second ?: R.string.profile_publish))
    }
    ProofProgress(state, scales)
}

@Composable
private fun ProofProgress(state: PublishState, scales: List<ProfileScale>) {
    if (state.solve != null) {
        OutlinedButton(onClick = {
            state.cancelling = true
            state.solve?.cancel()
        },
            Modifier.testTag("profile-cancel-proof"), enabled = !state.cancelling) {
            Text(stringResource(R.string.profile_cancel_proof))
        }
    }
    if (state.publishing) {
        Text(stringResource(R.string.profile_publishing), Modifier.testTag("profile-publish-status"))
    }
    if (state.nonce > 0uL) {
        Text(stringResource(R.string.profile_proof_nonce, "${state.nonce}"),
            Modifier.testTag("profile-proof-progress"))
    }
    state.failure?.let { (text, _) -> Text(stringResource(text), Modifier.testTag("profile-publish-error")) }
    state.published?.let { published ->
        Text(stringResource(R.string.profile_published_version, "${published.version}"),
            Modifier.testTag("profile-published-readback"))
        ProfilePreview(published.draft.draft(), scales, LocalContext.current, R.string.profile_published)
    }
}
