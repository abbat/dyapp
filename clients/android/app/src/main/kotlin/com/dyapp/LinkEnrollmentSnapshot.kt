package com.dyapp

import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSaved

/** Presentation only: the native review still validates every attempted completion. */
internal data class LinkEnrollmentSnapshot(
    val completed: Boolean = false,
    val comparison: LinkReviewOpening? = null,
    val failure: Throwable? = null,
    val offered: Boolean = false,
    val saved: LinkReviewSaved? = null,
    val confirmationAttempted: Boolean = false
) {
    val canFinish get() = !completed && (!offered || saved != null) &&
        (!confirmationAttempted || saved != null) && (failure == null || failure is LinkReviewException.Failed)
    val canPrepare get() = !completed && (
        failure is LinkReviewException.ReviewRequired ||
            (offered || confirmationAttempted) && failure is LinkReviewException.Failed
        )
    val canConfirm get() = !completed && offered && comparison != null && saved == null &&
        (failure == null || failure is LinkReviewException.Selection || failure is LinkReviewException.Profile)
}
