package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatMediaCacheException
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class MediaCacheErrorTests {
    @get:Rule
    val compose = createComposeRule()

    private fun replace(text: String) {
        compose.cacheReady("cache-limit").performScrollTo().performTextReplacement(text)
    }

    @Test
    fun unknownReadDisablesMutationsAndRetryDoesNotWriteADefault() {
        val fail = AtomicBoolean(true)
        val changes = AtomicInteger()
        val calls = MediaCacheCalls(read = {
            check(!fail.get()) { "controlled read failure" }
            cacheSettings()
        }, change = { _, _ ->
            changes.incrementAndGet()
            error("unexpected mutation")
        })
        compose.setContent { MediaCacheTestHost("owner", calls) }
        compose.cacheReady("cache-retry")
        for (tag in listOf("cache-enabled", "cache-limit", "cache-save", "cache-clear")) {
            compose.onNodeWithTag(tag).assertIsNotEnabled()
        }
        assertEquals(0, changes.get())
        fail.set(false)
        compose.cacheClick("cache-retry")
        compose.cacheReady("cache-limit")
        assertEquals("256", compose.cacheDraft())
        assertEquals(0, changes.get())
    }

    @Test
    fun indeterminateWriteRequiresReadbackAndRetainsTheLimitDraft() {
        val stored = AtomicReference(cacheSettings())
        val changes = AtomicInteger()
        val calls = MediaCacheCalls(read = { stored.get() }, change = { _, _ ->
            changes.incrementAndGet()
            stored.set(cacheSettings("1.5", 1_572_864uL))
            error("controlled failure after native commit")
        })
        compose.setContent { MediaCacheTestHost("owner", calls) }
        replace("1.5")
        compose.cacheClick("cache-save")
        compose.cacheReady("cache-retry")
        assertEquals("1.5", compose.cacheDraft())
        compose.onNodeWithTag("cache-enabled").assertIsNotEnabled()
        compose.onNodeWithTag("cache-save").assertIsNotEnabled()
        compose.cacheClick("cache-retry")
        compose.cacheReady("cache-limit")
        assertEquals("1.5", compose.cacheDraft())
        compose.onNodeWithTag("cache-save").assertIsNotEnabled()
        assertEquals(1, changes.get())
    }

    @Test
    fun invalidInputIsSentUnchangedToCoreAndKeepsTheEditorUsable() {
        val captured = AtomicReference<uniffi.dyapp.ChatMediaCacheEdit>()
        val calls = MediaCacheCalls(read = { cacheSettings() }, change = { edit, _ ->
            captured.set(edit)
            throw ChatMediaCacheException.InvalidLimit()
        })
        compose.setContent { MediaCacheTestHost("owner", calls) }
        replace("not a number")
        compose.cacheClick("cache-save")
        compose.awaitNode(hasTestTag("cache-invalid"))
        compose.cacheReady("cache-limit")
        assertEquals("not a number", compose.cacheDraft())
        assertEquals(uniffi.dyapp.ChatMediaCacheEdit.Budget("not a number"), captured.get())
        compose.onNodeWithTag("cache-error").assertDoesNotExist()
    }
}
