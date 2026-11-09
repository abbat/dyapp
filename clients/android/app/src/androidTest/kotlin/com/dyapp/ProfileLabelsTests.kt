package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.profileScales

class ProfileLabelsTests {
    @Test
    fun everyRuntimeScaleAndChoiceHasExactlyOneTranslatedResource() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val used = mutableSetOf<Int>()
        for (scale in profileScales()) {
            val keys = listOf("field.${scale.key}") + scale.choices.map { "value.${scale.key}.${it.key}" }
            for (key in keys) {
                val id = requireNotNull(profileLabelResource(key)) { "Missing profile label: $key" }
                assertEquals("profile_" + key.replace('.', '_'), context.resources.getResourceEntryName(id))
                check(context.getString(id).isNotBlank()) { "Empty profile label: $key" }
                check(used.add(id)) { "Duplicate profile resource: $key" }
            }
        }
        val defined = R.string::class.java.fields
            .filter { it.name.startsWith("profile_field_") || it.name.startsWith("profile_value_") }
            .map { it.getInt(null) }
            .toSet()
        assertEquals(defined, used)
    }

    @Test
    fun unknownMetadataKeepsTheRawKeyFallback() {
        assertNull(profileLabelResource("field.unknown"))
        assertNull(profileLabelResource("value.gender.unknown"))
    }
}
