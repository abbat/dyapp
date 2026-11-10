package com.dyapp

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProfileEditorCoreTests {
    @Test
    fun heightUsesTheNativeProfileBoundsAndBlankClears() {
        assertEquals(emptyMap<String, Int>(), parsedHeight("  "))
        assertEquals(mapOf(HEIGHT to 50), parsedHeight(" 50 "))
        assertEquals(mapOf(HEIGHT to 272), parsedHeight("272"))
        assertNull(parsedHeight("49"))
        assertNull(parsedHeight("273"))
        assertNull(parsedHeight("4294967296"))
    }

    @Test
    fun seekingUsesNativeValidationAndOrdering() {
        val inputs = SeekingInputs(ProfileSeeking())
        inputs.genders = setOf(2u, 1u)
        inputs.ageFrom.value = " 18 "
        inputs.ageTo.value = "120"
        assertEquals(ProfileSeeking(listOf(1u, 2u), 18u, 120u), inputs.parsed())
        inputs.ageTo.value = "17"
        assertNull(inputs.parsed())
    }

    @Test
    fun savedNotLookingShowsTheNormalizedNativeValue() {
        val inputs = SeekingInputs(ProfileSeeking())
        inputs.genders = setOf(999u)
        inputs.ageFrom.value = "invalid"
        inputs.ageTo.value = "-1"
        inputs.notLooking = true
        val saved = requireNotNull(inputs.parsed())
        assertEquals(ProfileSeeking(notLooking = true), saved)
        inputs.showSaved(saved)
        assertEquals(emptySet<UInt>(), inputs.genders)
        assertEquals("", inputs.ageFrom.value)
        assertEquals("", inputs.ageTo.value)
        assertTrue(inputs.notLooking)
    }
}
