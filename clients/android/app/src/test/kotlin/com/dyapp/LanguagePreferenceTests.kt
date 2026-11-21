package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LanguagePreferenceTests {
    private val available = listOf("en", "ru", "de", "fr", "zh-Hans")

    @Test
    fun savedChoicesUseTheCatalogueWithoutAFixedLanguageCount() {
        available.forEach { tag ->
            assertEquals(tag, selectedLanguage(tag, available))
            assertEquals(tag, selectedLanguage(tag.uppercase(), available))
        }
    }

    @Test
    fun legacyChoicesAndRemovedCataloguesHaveExplicitFallbacks() {
        assertEquals("en", selectedLanguage("EN", available))
        assertEquals("ru", selectedLanguage("RU", available))
        listOf(null, "", "SYSTEM", "unknown").forEach {
            assertNull(selectedLanguage(it, available))
        }
        assertNull(selectedLanguage("RU", listOf("en", "fr")))
        assertNull(selectedLanguage("EN", emptyList()))
    }
}
