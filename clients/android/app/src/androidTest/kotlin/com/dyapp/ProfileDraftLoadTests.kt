package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import java.io.File
import java.util.UUID

class ProfileDraftLoadTests {
    private val roots = mutableListOf<File>()

    @After
    fun cleanUp() {
        for (root in roots) {
            Slots(root).list().forEach { PlatformKeyStore.delete(identityAlias(it.id)) }
            root.deleteRecursively()
        }
    }

    @Test
    fun legacyJsonIsIgnoredWithoutCreatingACoreDraftOrChangingFiles() {
        for (json in listOf("{\"gender\":1}", "not JSON")) {
            val opened = ready()
            val legacy = File(opened.slot.dir, "profile-draft.json").apply { writeText(json) }
            assertEquals(ProfileDraft(), loadDraft(opened))
            assertEquals(json, legacy.readText())
            assertFalse(File(opened.slot.dir, "client.db").exists())
        }
    }

    @Test
    fun savedCoreDraftWinsAndLegacyFileStaysUntouched() {
        val opened = ready()
        val expected = ProfileDraft(values = mapOf(GENDER to 1), name = "Current")
        saveDraft(opened, expected)
        val legacy = File(opened.slot.dir, "profile-draft.json").apply { writeText("not JSON") }
        assertEquals(expected, loadDraft(opened))
        assertEquals("not JSON", legacy.readText())
    }

    @Test
    fun corruptDatabaseNeverFallsBackToLegacyOrEmptyDraft() {
        val opened = ready()
        val corrupt = "not a SQLite database"
        val database = File(opened.slot.dir, "client.db").apply { writeText(corrupt) }
        val legacy = File(opened.slot.dir, "profile-draft.json").apply { writeText("{\"gender\":1}") }
        assertThrows(CoreException::class.java) { loadDraft(opened) }
        assertEquals(corrupt, database.readText())
        assertEquals("{\"gender\":1}", legacy.readText())
    }

    @Test
    fun changedIdentityFailsBeforeAnyDraftFallback() {
        val opened = ready()
        val mismatched = Opened.Ready(opened.slot, "different peer")
        val failure = assertThrows(CoreException::class.java) { loadDraft(mismatched) }
        assertTrue(failure.toString().contains("The identity key changed"))
        assertFalse(File(opened.slot.dir, "client.db").exists())
    }

    private fun ready(): Opened.Ready {
        val id = UUID.randomUUID().toString()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "draft-loader-$id")
        roots += root
        return Slots(root).create()
    }
}
