package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class RemovalReadTests {
    @get:Rule
    val compose = createComposeRule()

    private fun show(fixture: NativeSlotFixture, opened: Opened.Ready) {
        compose.setContent {
            RemoveDialog(fixture.slots, opened, { error("Read-only fixture must not remove an identity") }, {})
        }
    }

    @Test
    fun corruptPublicationStorageCannotOfferRemovalAsIfItWereUnpublished() {
        NativeSlotFixture().use { fixture ->
            val opened = fixture.slots.create()
            val database = File(opened.slot.dir, "client.db")
            val corrupt = ByteArray(4096) { 0x5a }
            database.writeBytes(corrupt)
            show(fixture, opened)
            compose.awaitNode(hasTestTag("remove-read-failed")).assertExists()
            compose.onNodeWithTag("remove-confirm").assertDoesNotExist()
            compose.onNodeWithTag("remove-action").assertDoesNotExist()
            compose.onNodeWithTag("remove-cancel").assertExists()
            assertTrue(PlatformKeyStore.exists(identityAlias(opened.slot.id)))
            assertEquals(opened.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            assertArrayEquals(corrupt, database.readBytes())
        }
    }

    @Test
    fun aFreshUnpublishedIdentityStillRequiresConfirmationWithoutCreatingADatabase() {
        NativeSlotFixture().use { fixture ->
            val opened = fixture.slots.create()
            val database = File(opened.slot.dir, "client.db")
            assertFalse(database.exists())
            show(fixture, opened)
            compose.awaitNode(hasTestTag("remove-confirm")).assertExists()
            compose.onNodeWithTag("remove-action").assertIsNotEnabled()
            compose.onNodeWithTag("remove-read-failed").assertDoesNotExist()
            assertFalse(database.exists())
            assertTrue(PlatformKeyStore.exists(identityAlias(opened.slot.id)))
        }
    }
}
