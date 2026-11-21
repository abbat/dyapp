package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.StickerPickerEntry
import java.util.concurrent.atomic.AtomicInteger

class StickerPickerPresentationTests {
    private fun choice() = StickerPickerChoice(
        ByteArray(32),
        "Pack",
        StickerPickerEntry(17u, ":)", 2u, 1u, ByteArray(8))
    )

    private fun page(entry: StickerPickerChoice) = StickerPickerData.Choices(listOf(entry), 37u)

    @Test
    fun equalContentRefreshRetiresOldChoicesAndCommittedQueuedSendCannotRepeat() = runBlocking {
        val first = choice()
        val next = choice()
        var data = page(first)
        val sends = AtomicInteger()
        val calls = StickerPickerCalls({ data }, {
            assertSame(next, it)
            sends.incrementAndGet()
            ChatSubmission.Queued(byteArrayOf(1), "offline")
        })
        val state = StickerPickerPresentation()
        state.refresh(calls)
        val old = checkNotNull(state.view)
        data = page(next)
        state.refresh(calls)
        val current = checkNotNull(state.view)
        state.send(calls, old, first)
        state.send(calls, current, first)
        assertEquals(0, sends.get())
        state.send(calls, current, next)
        assertTrue(state.submitted is ChatSubmission.Queued)
        state.send(calls, current, next)
        assertEquals(1, sends.get())
        assertFalse(state.sendFailed)
    }

    @Test
    fun duplicateAndRetiredOpeningCannotSendOrPublishLateNativeCommitment() = runBlocking {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val entry = choice()
        val sends = AtomicInteger()
        val calls = StickerPickerCalls({ page(entry) }, {
            sends.incrementAndGet()
            entered.complete(Unit)
            withContext(NonCancellable) { release.await() }
            ChatSubmission.Queued(byteArrayOf(1), "offline")
        })
        val state = StickerPickerPresentation()
        state.refresh(calls)
        val shown = checkNotNull(state.view)
        val job = launch(start = CoroutineStart.UNDISPATCHED) { state.send(calls, shown, entry) }
        try {
            withTimeout(10_000) { entered.await() }
            state.send(calls, shown, entry)
            state.retire()
        } finally { release.complete(Unit) }
        withTimeout(10_000) { job.join() }
        state.send(calls, shown, entry)
        assertEquals(1, sends.get())
        assertNull(state.submitted)
        assertFalse(state.busy)
    }

    @Test
    fun nativeCursorsAndBackHistorySurviveReadFailureWithoutReenablingOldChoices() = runBlocking {
        val entry = choice()
        val seen = mutableListOf<StickerPickerQuery>()
        var fail = false
        val calls = StickerPickerCalls({
            seen += it
            check(!fail) { "controlled page read failure" }
            page(entry)
        }, { error("unexpected send") })
        val state = StickerPickerPresentation()
        state.refresh(calls)
        val initial = checkNotNull(state.view)
        val next = StickerPickerQuery.Choices(ByteArray(32) { 9 }, 37u)
        fail = true
        state.navigate(calls, next, initial)
        val failed = checkNotNull(state.view)
        assertNull(failed.data)
        assertTrue(state.hasPrevious)
        state.send(calls, initial, entry)
        fail = false
        state.refresh(calls)
        assertSame(next, seen.last())
        state.back(calls, checkNotNull(state.view))
        assertSame(initial.query, seen.last())
        assertFalse(state.hasPrevious)
    }
}
