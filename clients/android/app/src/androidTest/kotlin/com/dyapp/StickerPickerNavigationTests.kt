package com.dyapp

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class StickerPickerNavigationTests {
    @Test
    fun onlyTheCurrentOpeningCanManageAndOnlyItsCompletionCanReturn() {
        val state = StickerPickerNavigation()
        state.open(false)
        assertNull(state.opening)
        state.open(true)
        val first = checkNotNull(state.opening)
        state.open(true)
        assertSame(first, state.opening)
        assertTrue(state.manage(first, 17))
        state.open(true)
        state.resume(17, true)
        assertNull(state.opening)
        state.resume(18, true)
        val second = checkNotNull(state.opening)
        assertNotSame(first, second)
        state.close(first)
        assertFalse(state.manage(first, 18))
        assertSame(second, state.opening)
        state.close(second)
        assertNull(state.opening)
    }

    @Test
    fun managerReturnCannotReopenAConversationThatNoLongerAllowsComposing() {
        val state = StickerPickerNavigation()
        state.open(true)
        assertTrue(state.manage(checkNotNull(state.opening), 0))
        state.resume(1, false)
        assertNull(state.opening)
        assertFalse(state.waitingForManager)
    }
}
