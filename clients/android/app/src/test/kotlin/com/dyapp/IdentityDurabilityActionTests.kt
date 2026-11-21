package com.dyapp

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

class IdentityDurabilityActionTests {
    @Test
    fun committedUncertaintyKeepsSelectionAndASyncedSuccessorClearsTheDetail() = runBlocking {
        val selected = Opened.Ready(Slot("committed", File("presentation-only"), SlotState.READY), "a".repeat(64))
        var detail: String? = "unconfirmed"
        val controller = IdentityActions(this) { action -> Shown(action(), durabilityDetail = detail) }
        val calls = AtomicInteger()
        val action = {
            calls.incrementAndGet()
            selected
        }
        withTimeout(5_000) { controller.initialize(action).join() }
        assertSame(selected, controller.state.value.shown?.opened)
        assertEquals("unconfirmed", controller.state.value.shown?.durabilityDetail)
        assertFalse(controller.state.value.loading)
        assertEquals(1, calls.get())
        detail = null
        withTimeout(5_000) { checkNotNull(controller.perform(action)).join() }
        assertSame(selected, controller.state.value.shown?.opened)
        assertNull(controller.state.value.shown?.durabilityDetail)
        assertEquals(2, calls.get())
    }
}
