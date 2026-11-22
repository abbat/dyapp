package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.Rgba
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** Controlled read completion, not a test of codec or network policy. */
internal class GatedLinkPhoto(val snapshot: LinkReviewSnapshot = linkReviewFixture()) {
    val entered = CompletableDeferred<Unit>()
    val release = CountDownLatch(1)
    val closes = AtomicInteger()
    val pixels = byteArrayOf(10, 20, 30, -1, 50, 60, 70, -1)
    val review = LinkEnrollmentReview(
        snapshot,
        { throw LinkReviewException.ReviewRequired() },
        { error("Unexpected confirmation") },
        { snapshot.remote },
        { _, _ ->
            entered.complete(Unit)
            check(release.await(5, TimeUnit.SECONDS)) { "Thumbnail read was not released" }
            Rgba(2u, 1u, pixels.copyOf())
        },
        { closes.incrementAndGet() }
    )
}
