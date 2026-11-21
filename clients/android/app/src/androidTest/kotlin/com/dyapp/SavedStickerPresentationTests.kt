package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.SavedStickerPackEntry
import uniffi.dyapp.SavedStickerPackImport
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.SavedStickerPackState
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger

class SavedStickerPresentationTests {
    private fun entry() = SavedStickerPackEntry(
        ByteArray(32) { 1 },
        19uL,
        SavedStickerPackState.Unreadable("controlled unreadable content")
    )

    private fun page(entry: SavedStickerPackEntry? = null) =
        SavedStickerPackPage(listOfNotNull(entry), null, if (entry == null) 0uL else 19uL, 0uL)

    private fun inert(initial: SavedStickerPackPage) = SavedStickerCalls(
        { initial },
        { error("unexpected import") },
        { error("unexpected removal") }
    )

    @Test
    fun cancelAndRepeatedConfirmationPreserveExactSelection() = runBlocking {
        val selected = entry()
        var current = page(selected)
        val writes = AtomicInteger()
        val calls = SavedStickerCalls({ current }, { error("unexpected import") }, { id ->
            assertArrayEquals(selected.packId, id)
            writes.incrementAndGet()
            current = page()
            true
        })
        val state = SavedStickerPresentation()
        state.refresh(calls)
        state.askRemove(selected)
        val old = checkNotNull(state.pending)
        state.cancelRemove()
        state.confirmRemove(calls, old)
        assertEquals(0, writes.get())
        state.askRemove(selected)
        val current = checkNotNull(state.pending)
        state.confirmRemove(calls, old)
        assertEquals(0, writes.get())
        state.confirmRemove(calls, current)
        state.confirmRemove(calls, current)
        assertEquals(1, writes.get())
        assertTrue(checkNotNull(state.page).entries.isEmpty())
        state.askRemove(selected)
        assertNull(state.pending)
    }

    @Test
    fun uncertainRemovalRequiresReadbackAndNeverRepeatsTheUnlink() = runBlocking {
        val selected = entry()
        val initial = page(selected)
        var unreadable = false
        val writes = AtomicInteger()
        val calls = SavedStickerCalls(
            { if (unreadable) throw IOException("controlled read failure") else page() },
            { error("unexpected import") },
            {
                writes.incrementAndGet()
                unreadable = true
                throw IOException("after unlink")
            }
        )
        val state = SavedStickerPresentation()
        state.refresh(inert(initial))
        state.askRemove(selected)
        val confirmation = checkNotNull(state.pending)
        state.confirmRemove(calls, confirmation)
        assertSame(initial, state.page)
        assertTrue(state.readFailed && state.operationFailed)
        assertFalse(state.editable)
        state.confirmRemove(calls, confirmation)
        assertEquals(1, writes.get())
        unreadable = false
        state.refresh(calls)
        assertTrue(checkNotNull(state.page).entries.isEmpty())
        assertTrue(state.editable)
    }

    @Test
    fun closingDuringPickedFileReadPreventsNativeImportAdmission() = runBlocking {
        val initial = page()
        val state = SavedStickerPresentation()
        val saves = AtomicInteger()
        val calls = SavedStickerCalls({ initial }, {
            saves.incrementAndGet()
            SavedStickerPackImport(ByteArray(32), true)
        }, { error("unexpected removal") })
        state.refresh(calls)
        retireDuringRead(state, calls, checkNotNull(state.requestFile()))
        assertSame(initial, state.page)
        assertFalse(state.editable)
        assertEquals(0, saves.get())
    }

    private suspend fun retireDuringRead(
        state: SavedStickerPresentation,
        calls: SavedStickerCalls,
        ticket: Any
    ) = coroutineScope {
        val started = CompletableDeferred<Unit>()
        val finish = CompletableDeferred<Unit>()
        val pending = launch(start = CoroutineStart.UNDISPATCHED) {
            state.importPicked(calls, ticket) {
                withContext(NonCancellable) {
                    started.complete(Unit)
                    finish.await()
                }
                byteArrayOf(1)
            }
        }
        try {
            withTimeout(10_000) { started.await() }
            state.retire()
        } finally {
            finish.complete(Unit)
            withTimeout(10_000) { pending.join() }
        }
    }

    @Test
    fun cancelAndRetiredPickerTicketsCannotImportIntoANewerOpening() = runBlocking {
        val state = SavedStickerPresentation()
        val saves = AtomicInteger()
        val calls = SavedStickerCalls({ page() }, {
            saves.incrementAndGet()
            SavedStickerPackImport(ByteArray(32), true)
        }, { error("unexpected removal") })
        state.refresh(calls)
        val reads = AtomicInteger()
        val load: suspend () -> ByteArray = {
            reads.incrementAndGet()
            byteArrayOf(1)
        }
        val old = checkNotNull(state.requestFile())
        state.importPicked(calls, old, null)
        val current = checkNotNull(state.requestFile())
        state.importPicked(calls, old, load)
        assertEquals(0, reads.get())
        assertEquals(0, saves.get())
        state.importPicked(calls, current, load)
        state.importPicked(calls, current, load)
        assertEquals(1, reads.get())
        assertEquals(1, saves.get())
        assertTrue(checkNotNull(state.imported).added)
    }
}
