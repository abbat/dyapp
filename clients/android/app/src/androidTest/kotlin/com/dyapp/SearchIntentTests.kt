package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.SearchEmployer
import uniffi.dyapp.profileScales

class SearchIntentTests {
    private fun filters() = SearchFilters(profileScales()).apply {
        offered = listOf(36u, 4u, 19u, 20u, 46u, 1001u, 53u, 54u)
        picked["gender"] = 1
        place.country = "DE"
        place.place = 7
        interest = " MUSIC "
        ageMin = "20"
        ageMax = "40"
        heightMin = "150"
        heightMax = "190"
        employer.kind = EmployerKind.SAME
        employer.company = " Original Co "
        mutual = true
    }

    @Test
    fun everyFormValueIsFrozenBeforeLaterEditsAndOfferedFieldChanges() {
        val filters = filters()
        val admitted = filters.captureIntent().getOrThrow()
        // Copy the admitted payload once; a second age parse could cross UTC midnight.
        val expected = admitted.choices.map { it.copy(ints = it.ints.toList(), strings = it.strings.toList()) }
        filters.offered = emptyList()
        filters.picked["gender"] = 2
        filters.place.pickCountry("US")
        filters.place.place = 9
        filters.interest = "other"
        filters.ageMin = "30"
        filters.ageMax = "50"
        filters.heightMin = "170"
        filters.heightMax = "200"
        filters.employer.kind = EmployerKind.EXCLUDE
        filters.employer.company = "Later Co"
        filters.mutual = false
        assertEquals(expected, admitted.choices)
        assertEquals(SearchEmployer.Same("Original Co"), admitted.employer)
        assertTrue(admitted.mutual)
        val next = filters.captureIntent().getOrThrow()
        assertTrue(next.choices.isEmpty())
        assertEquals(SearchEmployer.Exclude(listOf("Later Co")), next.employer)
        assertFalse(next.mutual)
    }

    @Test
    fun invalidAdmissionCannotBeRepairedByEditsBeforeBackendReadiness() {
        val filters = filters()
        filters.interest = "music\nother"
        val admitted = filters.captureIntent()
        filters.interest = "music"
        assertTrue(admitted.exceptionOrNull().toString().contains("invalid search interest"))
        assertTrue(filters.captureIntent().isSuccess)
    }

    @Test
    fun capturedMutualChoiceStillRequiresASavedOwnGenderAndExactAge() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val filters = filters()
            val admitted = filters.captureIntent().getOrThrow()
            filters.mutual = false
            val error = assertThrows(IllegalArgumentException::class.java) { admitted.conditions(owner) }
            assertEquals("mutual unavailable", error.message)
            val next = filters.captureIntent().getOrThrow()
            assertEquals(next.choices, next.conditions(owner))
        }
    }
}
