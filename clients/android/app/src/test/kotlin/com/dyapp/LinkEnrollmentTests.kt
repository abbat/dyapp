package com.dyapp

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.LinkReviewException

class LinkEnrollmentTests {
    @Test
    fun typedRecoveryAndAuthorizationRefusalsNeverOfferOrdinaryRetry() {
        val refusals = listOf(
            LinkReviewException.ReviewRequired(),
            LinkReviewException.PrivateStateReviewRequired(),
            LinkReviewException.EnrollmentAuthorizationRequired(),
            LinkReviewException.Revoked(),
            LinkReviewException.DeviceNotListed(),
            LinkReviewException.Selection("missing choice"),
            LinkReviewException.Profile("invalid fields"),
            IllegalStateException("unknown native boundary")
        )
        for (failure in refusals) {
            assertFalse(failure.javaClass.simpleName, LinkEnrollmentSnapshot(failure = failure).canFinish)
        }
    }

    @Test
    fun explicitBackendRetryUsesTheTypedBoundaryRatherThanDiagnosticWords() {
        assertTrue(LinkEnrollmentSnapshot().canFinish)
        for (detail in listOf("offline", "revoked", "fresh review required")) {
            val failure = LinkReviewException.Failed(detail)
            assertTrue(LinkEnrollmentSnapshot(failure = failure).canFinish)
            assertFalse(LinkEnrollmentSnapshot(completed = true, failure = failure).canFinish)
        }
    }
}
