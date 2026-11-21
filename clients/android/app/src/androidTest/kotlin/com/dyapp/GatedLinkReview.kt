package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSnapshot
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Native-confirmation-shaped gate; no network timing or rendered busy frame is required. */
internal class GatedLinkReview(snapshot: LinkReviewSnapshot) {
    val entered = CompletableDeferred<Unit>()
    val release = CountDownLatch(1)
    val submitted = AtomicReference<List<LinkReviewChoice>>()
    val closes = AtomicInteger()
    val review = LinkEnrollmentReview(
        snapshot,
        { false },
        { choices ->
            entered.complete(Unit)
            check(release.await(5, TimeUnit.SECONDS)) { "Confirmation was not released" }
            submitted.set(choices)
            LinkReviewSaved(snapshot.remote, snapshot.local, hidden = true, pending = true)
        },
        { snapshot.remote },
        { closes.incrementAndGet() }
    )
}
