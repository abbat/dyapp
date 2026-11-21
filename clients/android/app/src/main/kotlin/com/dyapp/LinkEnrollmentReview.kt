package com.dyapp

import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileLinkRecovery

/** Original immutable native comparison; retaining it holds no operation lease or plaintext keys. */
internal class LinkEnrollmentReview(
    val snapshot: LinkReviewSnapshot,
    val resume: () -> Boolean,
    val confirm: (List<LinkReviewChoice>) -> LinkReviewSaved,
    private val release: () -> Unit
) : AutoCloseable {
    override fun close() = release()
}

/** Bind existing credential access to the actual installed destination, never the current UI selection. */
internal fun linkEnrollmentReview(registry: MobileIdentityRegistry, id: String, peer: String): LinkEnrollmentReview =
    MobileLinkRecovery(registry, id, peer, PlatformKeyStore.existing(identityAlias(id))).use { recovery ->
        val review = recovery.prepare()
        runCatching {
            LinkEnrollmentReview(review.snapshot(), review::resumeLink, review::confirm, review::close)
        }.onFailure { review.close() }.getOrThrow()
    }
