package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BirthPartsTests {
    @Test
    fun optionalPartsStayAbsentAndCanBeCleared() {
        assertEquals(emptyMap<String, Int>(), parseBirthParts("", "", ""))
        assertEquals(mapOf("birth_month" to 2, "birth_day" to 29), parseBirthParts("", "2", "29"))
        assertEquals(mapOf("birth_year" to 2000), parseBirthParts("2000", "", ""))
    }

    @Test
    fun impossibleDatesAndZeroAreRejected() {
        assertNull(parseBirthParts("2023", "2", "29"))
        assertNull(parseBirthParts("", "4", "31"))
        assertNull(parseBirthParts("0", "", ""))
        assertNull(parseBirthParts("", "13", ""))
        assertNull(parseBirthParts("", "2", "not a day"))
        assertEquals(mapOf("birth_year" to 2024, "birth_month" to 2, "birth_day" to 29),
            parseBirthParts("2024", "2", "29"))
    }
}
