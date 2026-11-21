package com.dyapp

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatSubmission

internal fun forwardSource() = ChatForwardSource("source", byteArrayOf(1), "author", ByteArray(16) { 2 })

class ChatForwardPresentationTests {
    @Test
    fun acceptedAndQueuedCommitOnceAndCannotReuseOldNavigation() = runBlocking {
        verifyCommitted(ChatSubmission.Accepted(ByteArray(16)))
        verifyCommitted(ChatSubmission.Queued(ByteArray(16), "offline"))
    }

    private suspend fun verifyCommitted(outcome: ChatSubmission) {
        val f = ForwardStateFixture()
        f.outcome = outcome
        val opening = f.ready()
        f.state.choose(opening, "two")
        f.state.send(opening, f.calls)
        f.state.send(opening, f.calls)
        val receipt = checkNotNull(f.state.submitted)
        assertSame(outcome, receipt.result)
        assertSame(f.source, f.sentSource)
        assertEquals("two", f.sentTarget)
        val opened = mutableListOf<String>()
        f.state.complete(receipt, opened::add)
        f.ask()
        f.state.complete(receipt, opened::add)
        f.state.cancel(opening)
        assertNotNull(f.state.opening)
        assertEquals(listOf("two"), opened)
        assertEquals(1, f.sends)
    }

    @Test
    fun readFailureDoesNotBecomeEmptyChoicesOrAdmitSending() = runBlocking {
        val f = ForwardStateFixture()
        f.failRead = true
        val opening = f.ask()
        f.state.refresh(opening, f.calls)
        assertNull(f.state.targets)
        assertNull(f.state.chosen)
        assertNotNull(f.state.failure)
        f.state.send(opening, f.calls)
        assertEquals(0, f.sends)
        f.failRead = false
        f.state.refresh(opening, f.calls)
        assertEquals(listOf("one", "two"), f.state.targets)
        assertNull(f.state.failure)
    }

    @Test
    fun precommitFailureKeepsTheSelectionAndExplicitRetryCanCommit() = runBlocking {
        val f = ForwardStateFixture()
        val opening = f.ready()
        f.state.choose(opening, "not offered")
        assertEquals("one", f.state.chosen)
        f.failSend = true
        f.state.send(opening, f.calls)
        assertNull(f.state.submitted)
        assertEquals("one", f.state.chosen)
        assertNotNull(f.state.failure)
        f.failSend = false
        f.state.send(opening, f.calls)
        assertEquals(2, f.sends)
        assertNotNull(f.state.submitted)
    }

    @Test
    fun cancelledOpeningCannotSelectRefreshSendOrCancelItsReplacement() = runBlocking {
        val f = ForwardStateFixture()
        val old = f.ask()
        f.state.cancel(old)
        val current = f.ready()
        f.state.choose(old, "two")
        f.state.refresh(old, f.calls)
        f.state.send(old, f.calls)
        f.state.cancel(old)
        assertSame(current, f.state.opening)
        assertEquals("one", f.state.chosen)
        assertEquals(1, f.reads)
        assertEquals(0, f.sends)
    }

    @Test
    fun retirementBetweenCommitAndNavigationCannotOpenTheDestination() = runBlocking {
        val f = ForwardStateFixture()
        val opening = f.ready()
        f.state.send(opening, f.calls)
        val receipt = checkNotNull(f.state.submitted)
        val opened = mutableListOf<String>()
        f.state.retire()
        f.state.complete(receipt, opened::add)
        assertEquals(emptyList<String>(), opened)
    }

    @Test
    fun retiredOwnerCannotReadSendOrOpenAnotherDialog() = runBlocking {
        val f = ForwardStateFixture()
        val opening = f.ready()
        f.state.retire()
        f.state.send(opening, f.calls)
        f.state.refresh(opening, f.calls)
        f.state.ask(f.source)
        assertNull(f.state.opening)
        assertEquals(1, f.reads)
        assertEquals(0, f.sends)
    }
}
