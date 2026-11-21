package com.dyapp

import kotlinx.coroutines.Deferred
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewSaved

/** Explicit recovery controls; saving fields and finishing device setup remain separate actions. */
internal class LinkEnrollmentActions(
    val finish: () -> Unit,
    val prepare: () -> Unit,
    val confirm: (LinkReviewOpening, List<LinkReviewChoice>) -> Deferred<Result<LinkReviewSaved>>?
)
