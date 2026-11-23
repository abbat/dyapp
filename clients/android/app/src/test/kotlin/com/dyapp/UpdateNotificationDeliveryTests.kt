package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateOffer
import java.io.IOException

class UpdateNotificationDeliveryTests {
    private val offer = MobileUpdateOffer(
        "2.0.0",
        MobileUpdateDelivery.PACKAGE,
        "https://github.com/abbat/dyapp/releases/tag/v2.0.0"
    )

    @Test
    fun deniedOrStoppedPlatformNeverConsumesAClaim() {
        deliverCachedUpdate({ false }, { error("Unadmitted claim") }, { error("Unadmitted post") })
    }

    @Test
    fun missingPlanNeverPostsOrRequestsAnotherClaim() {
        val events = mutableListOf<String>()
        deliverCachedUpdate(
            allowed = {
                events.add("allowed")
                true
            },
            claim = {
                events.add("claim")
                null
            },
            post = { error("No offer") }
        )
        assertEquals(listOf("allowed", "claim"), events)
    }

    @Test
    fun stoppedOrBlockedAfterCommitNeverPosts() {
        var allowed = true
        var claims = 0
        deliverCachedUpdate(
            allowed = { allowed },
            claim = {
                claims++
                allowed = false
                offer
            },
            post = { error("Blocked post") }
        )
        assertEquals(1, claims)
    }

    @Test
    fun onlyTheCommittedOfferReachesThePlatformAfterASecondGate() {
        val events = mutableListOf<String>()
        deliverCachedUpdate(
            {
                events.add("allowed")
                true
            },
            {
                events.add("commit")
                offer
            },
            {
                assertSame(offer, it)
                events.add("post")
            }
        )
        assertEquals(listOf("allowed", "commit", "allowed", "post"), events)
    }

    @Test
    fun failedCommitCannotReachThePlatform() {
        assertThrows(IOException::class.java) {
            deliverCachedUpdate({ true }, { throw IOException("Controlled failed commit") }, { error("Failed claim") })
        }
    }
}
