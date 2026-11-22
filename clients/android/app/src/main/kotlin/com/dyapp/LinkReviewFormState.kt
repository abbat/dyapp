package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSource

/** UI choices only; native preview and confirmation retain all validation and merge authority. */
internal class LinkReviewFormState(val opening: LinkReviewOpening, private val enabled: () -> Boolean) {
    private val selected = mutableStateMapOf<LinkReviewField, LinkReviewSource>()
    private var retired = false
    var preview by mutableStateOf<LinkReviewProfile?>(null)
        private set
    var failure by mutableStateOf<Throwable?>(null)
        private set
    var submitting by mutableStateOf(false)
        private set
    private var confirmed by mutableStateOf(false)
    var loadingPhotos by mutableStateOf(false)
    val present get() = !retired
    val available get() = present && !submitting && !confirmed && !loadingPhotos && enabled()
    val complete get() = opening.snapshot.differences.all { it in selected }

    fun source(field: LinkReviewField) = selected[field]
    fun choices() = opening.snapshot.differences.mapNotNull { field ->
        selected[field]?.let { LinkReviewChoice(field, it) }
    }
    fun choose(field: LinkReviewField, source: LinkReviewSource) {
        if (!available || field !in opening.snapshot.differences) return
        selected[field] = source
        preview = null
        failure = null
    }
    fun showPreview() {
        if (!available || !complete) return
        runCatching { opening.preview(choices()) }
            .onSuccess {
                preview = it
                failure = null
            }
            .onFailure { failure = it }
    }
    fun editPreview() { if (available) preview = null }
    fun beginConfirmation(actions: LinkEnrollmentActions) =
        if (available && complete) actions.confirm(opening, choices())?.also { submitting = true } else null
    fun confirmationCompleted(outcome: Result<LinkReviewSaved>) {
        if (!present) return
        submitting = false
        confirmed = outcome.isSuccess
    }
    fun retire() { retired = true }
}
