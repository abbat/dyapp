package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.profileScales

class SearchChoiceTests {
    @Test
    fun selectedPlaceSurvivesHiddenCountryAndPreservesAllIdBits() {
        val filters = SearchFilters(profileScales())
        filters.offered = listOf(20u)
        filters.place.country = "DE"
        // PlacePicker stores native UInt32 IDs through toInt(); restore the same bits.
        filters.place.place = UInt.MAX_VALUE.toInt()
        val conditions = filters.conditions()
        assertEquals(listOf(20u), conditions.map { it.field })
        assertEquals(listOf(listOf("4294967295")), conditions.map { it.strings })
    }

    @Test
    fun unselectedTextAndHiddenChoicesNeverBecomeConditions() {
        val filters = SearchFilters(profileScales())
        filters.picked["gender"] = 1
        filters.place.country = "DE"
        filters.place.text = "arbitrary city text"
        assertEquals(emptyList<Any>(), filters.conditions())
        filters.offered = listOf(20u)
        assertEquals(emptyList<Any>(), filters.conditions())
        filters.offered = listOf(36u)
        assertEquals(listOf(listOf(1L)), filters.conditions().map { it.ints })
        filters.picked["gender"] = 0
        assertEquals(emptyList<Any>(), filters.conditions())
    }

    @Test
    fun coreTrimsCountryAndCountryChangesInvalidatePlaceSelection() {
        val filters = SearchFilters(profileScales())
        filters.offered = listOf(4u, 20u)
        filters.place.country = "\uFEFFDE\u00A0"
        filters.place.place = 7
        assertEquals(listOf(listOf("DE"), listOf("7")), filters.conditions().map { it.strings })
        filters.place.pickCountry("US")
        assertEquals(listOf(listOf("US")), filters.conditions().map { it.strings })
    }
}
