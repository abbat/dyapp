package com.dyapp

import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSnapshot

/** Reference identity distinguishes equal fresh comparisons; the receiver owns the native handle. */
internal class LinkReviewOpening(
    val snapshot: LinkReviewSnapshot,
    private val render: (LinkReviewOpening, List<LinkReviewChoice>) -> LinkReviewProfile
) {
    fun preview(choices: List<LinkReviewChoice>): LinkReviewProfile = render(this, choices)
}
