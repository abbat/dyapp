package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Locale

class LanguageCatalogTests {
    @Test
    fun actualPackagedCatalogueHasCanonicalTagsAndReadableAutonyms() {
        val tags = InstrumentationRegistry.getInstrumentation().targetContext.packagedLanguages()
        assertTrue(tags.isNotEmpty())
        assertEquals(tags.size, tags.distinct().size)
        for (tag in tags) {
            assertEquals(tag, Locale.forLanguageTag(tag).toLanguageTag())
            assertTrue(languageName(tag).isNotBlank())
        }
        assertFalse(tags.contains("en-XA"))
        assertFalse(tags.contains("ar-XB"))
    }
}
