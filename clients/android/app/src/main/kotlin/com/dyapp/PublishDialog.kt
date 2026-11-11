package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileScale
import kotlin.reflect.KMutableProperty0

private fun blockText(block: String) = when (block) {
    "notAttested" -> R.string.profile_publish_blocked_not_attested
    "invalidBirthDate" -> R.string.profile_publish_blocked_invalid_birth_date
    "underage" -> R.string.profile_publish_blocked_underage
    else -> R.string.profile_eligibility_failed
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

// Reads the saved draft once per opening; a running attempt keeps the dialog up until it ends or fails.
@Composable
internal fun PublishDialog(
    state: PublishState,
    scales: List<ProfileScale>,
    read: () -> ProfileDraft,
    publish: () -> Unit,
) {
    val draft by rememberNativeRead(state, read = read)
    val eligibility = state.eligibility()
    AlertDialog(
        onDismissRequest = { if (!state.running) state.close() },
        title = { Text(stringResource(R.string.publish_title)) },
        text = { PublishDialogBody(state, draft, scales, eligibility) },
        confirmButton = {
            TextButton(
                onClick = publish,
                Modifier.testTag("profile-publish-confirm"),
                enabled = draft?.isSuccess == true && !state.running && eligibility == R.string.profile_eligible
            ) {
                Text(stringResource(state.failure?.second ?: R.string.publish_confirm))
            }
        },
        dismissButton = { PublishCancel(state) },
    )
}

@Composable
private fun PublishDialogBody(
    state: PublishState,
    draft: Result<ProfileDraft>?,
    scales: List<ProfileScale>,
    eligibility: Int,
) {
    Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(if (draft == null) Modifier else Modifier.testTag("profile-public-fields")) {
            if (draft == null) Text(stringResource(R.string.view_loading))
            draft?.fold(
                { ProfilePreview(it, scales, R.string.publish_intro) },
                { Text(stringResource(R.string.profile_read_failed)) }
            )
        }
        if (state.running) {
            PublishProgress(state)
        } else {
            Confirmation(state::adult, R.string.profile_confirm_adult, "profile-attested")
            Confirmation(state::public, R.string.profile_confirm_public, "profile-disclosure")
            Text(stringResource(eligibility), Modifier.testTag(ELIGIBILITY_TAG))
        }
        state.failure?.let { (text, _) ->
            Text(
                stringResource(text),
                Modifier.testTag("profile-publish-error")
                    .semantics { liveRegion = LiveRegionMode.Polite }
            )
        }
    }
}

// Only the proof can stop; the publication itself runs to its result.
@Composable
private fun PublishCancel(state: PublishState) {
    TextButton(onClick = {
        if (state.running) {
            state.cancelling = true
            state.solve?.cancel()
        } else {
            state.close()
        }
    }, Modifier.testTag("profile-cancel"), enabled = !state.cancelling && (!state.running || state.solve != null)) {
        Text(stringResource(R.string.remove_cancel))
    }
}

@Composable
private fun PublishProgress(state: PublishState) {
    Text(stringResource(R.string.publish_working), Modifier.semantics { liveRegion = LiveRegionMode.Polite })
    LinearProgressIndicator(Modifier.fillMaxWidth())
    if (state.nonce > 0uL) {
        Text(
            stringResource(R.string.profile_proof_nonce, "${state.nonce}"),
            Modifier.testTag("profile-proof-progress")
        )
    }
    if (state.publishing) {
        Text(stringResource(R.string.profile_publishing), Modifier.testTag("profile-publish-status"))
    }
}
