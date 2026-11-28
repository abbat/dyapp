package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ProfileSearchResult
import uniffi.dyapp.ProfileSearcher
import java.util.UUID

/** Actual published profiles and a slot-bound searcher on the suite's existing node. */
class NativeInterestSearchTests {
    @Test
    fun canonicalInterestFindsThePublishedPeerAndCompleteNonmatchingSearchIsEmpty() {
        workerTestPolicy()
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val seeker = fixture.slots.create()
            val suffix = UUID.randomUUID().toString()
            val tag = "\u00e9cole-$suffix"
            val tags = listOf(tag, "tea, coffee; music")
            saveDraft(owner, ProfileDraft(interests = tags))
            publishWorkerPeer(owner)
            publishWorkerPeer(seeker)
            seeker.profileSearcher().use { searcher ->
                val filters = SearchFilters(emptyList())
                val sample = completeSearch(searcher, filters)
                assertTrue("The suite node must offer interest search", 19u in sample.offered)
                filters.offered = sample.offered
                filters.interest = " E\u0301COLE-$suffix "
                val found = completeSearch(searcher, filters, owner.peerId)
                assertEquals(listOf(owner.peerId), found.profiles.map { it.peerId })
                assertEquals(tags, found.profiles.single().draft.interests)
                filters.interest = "missing-$suffix"
                assertTrue(completeSearch(searcher, filters).profiles.isEmpty())
            }
        }
    }
}

internal fun completeSearch(
    searcher: ProfileSearcher,
    filters: SearchFilters,
    expectedPeer: String? = null,
): ProfileSearchResult {
    var result: ProfileSearchResult? = null
    val transition = PeerTransition("completed native interest search for " + filters.interest)
    try {
        transition.await(
            ready = {
                result?.let { completed ->
                    !completed.partial && completed.failures.isEmpty() &&
                        (expectedPeer == null || completed.profiles.any { it.peerId == expectedPeer })
                } == true
            },
            advance = {
                result = searcher.searchSlot(filters.conditions(), filters.employer.employer(), 0u)
            }
        )
    } catch (failure: IllegalStateException) {
        val last = result
        throw IllegalStateException(
            "Interest search did not converge: partial=${last?.partial}, " +
                "failures=${last?.failures}, profiles=${last?.profiles?.size}",
            failure
        )
    }
    return requireNotNull(result)
}
