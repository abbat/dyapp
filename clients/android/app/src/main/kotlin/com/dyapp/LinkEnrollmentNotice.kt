package com.dyapp

import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.LinkReviewException

/** Refusals are typed; neither a diagnostic nor refresh grants device authorization. */
internal fun linkEnrollmentFailureText(failure: Throwable): Int = when (failure) {
    is LinkReviewException.ReviewRequired -> R.string.link_setup_review
    is LinkReviewException.PrivateStateReviewRequired -> R.string.link_setup_private_review
    is LinkReviewException.EnrollmentAuthorizationRequired -> R.string.link_setup_authorization
    is LinkReviewException.Revoked -> R.string.link_setup_revoked
    is LinkReviewException.DeviceNotListed -> R.string.link_setup_unlisted
    is LinkReviewException.Selection, is LinkReviewException.Profile -> R.string.link_setup_selection
    else -> R.string.link_setup_error
}

@Composable
internal fun LinkEnrollmentNotice(state: LinkEnrollmentSnapshot) {
    state.failure?.let { failure ->
        ErrorNotice(
            stringResource(linkEnrollmentFailureText(failure)),
            "link-setup-failure",
            failure.toString(),
            "link-setup-details",
            "link-setup-copy-details"
        )
    }
}

@Composable
internal fun LinkEnrollmentAction(snapshot: LinkReceiverSnapshot, actions: LinkReceiveActions) {
    if (!snapshot.enrollment.canFinish) return
    Button(actions.finish, Modifier.testTag("link-setup-finish").applicationClick(actions.finish), !snapshot.busy) {
        Text(stringResource(R.string.link_setup_finish))
    }
}
