package com.dyapp

import kotlinx.coroutines.Deferred
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.Rgba

/** Reference identity distinguishes equal fresh comparisons; the receiver owns the native handle. */
internal class LinkReviewOpening(
    val snapshot: LinkReviewSnapshot,
    private val render: (LinkReviewOpening, List<LinkReviewChoice>) -> LinkReviewProfile,
    private val load: (LinkReviewOpening, LinkReviewSource, UInt) -> Deferred<Result<Rgba>>?
) {
    fun preview(choices: List<LinkReviewChoice>): LinkReviewProfile = render(this, choices)
    fun thumbnail(source: LinkReviewSource, index: UInt): Deferred<Result<Rgba>>? = load(this, source, index)
}
