package com.dyapp

import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSnapshot

/** Presentation only: the native review still validates every attempted completion. */
internal data class LinkEnrollmentSnapshot(
    val completed: Boolean = false,
    val comparison: LinkReviewSnapshot? = null,
    val failure: Throwable? = null
) {
    val canFinish get() = !completed && (failure == null || failure is LinkReviewException.Failed)
}
