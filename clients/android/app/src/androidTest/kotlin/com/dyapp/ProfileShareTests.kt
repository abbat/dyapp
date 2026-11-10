package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission

class ProfileShareTests {
    @Test
    fun sharingCompletionUsesNativeCommitmentRatherThanStatusDetails() {
        val refused = ProfileShareForm()
        refused.completeSubmission(Result.failure(IllegalStateException("Queued")), "target")
        assertNull(refused.sentTo)
        assertEquals(R.string.chat_send_failed, refused.status?.text)
        for ((outcome, status) in listOf(
            ChatSubmission.Accepted(byteArrayOf(1)) to R.string.profile_shared,
            ChatSubmission.Queued(byteArrayOf(2), "disk") to R.string.chat_queued,
        )) {
            val form = ProfileShareForm()
            form.completeSubmission(Result.success(outcome), "target")
            assertEquals("target", form.sentTo)
            assertEquals(status, form.status?.text)
            assertNull(form.status?.details)
        }
    }

    @Test
    fun sharingOffersOnlyOpenChatsOtherThanTheSharedProfile() {
        val rows = ChatState.entries.mapIndexed { index, state ->
            ChatRow("peer-$index", state, null, 0uL, false)
        }
        val open = rows.single { it.state == ChatState.OPEN }
        assertEquals(listOf(open), profileShareTargets(rows, "another-peer"))
        assertEquals(emptyList<ChatRow>(), profileShareTargets(rows, open.peerId))
        assertEquals(emptyList<ChatRow>(), profileShareTargets(emptyList(), "another-peer"))
    }
}
