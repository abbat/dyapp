package com.dyapp

import androidx.compose.runtime.mutableStateOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

class ReceiveWizardNavigationTests {
    private fun ready() = Opened.Ready(Slot("wizard", File("presentation-only"), SlotState.READY), "peer")

    @Test
    fun creationIsExplicitOnceAndAnOldSetupCannotCreateAfterAnEqualReturn() = runBlocking {
        val actions = IdentityActions(this) { Shown(it()) }
        withTimeout(5_000) { actions.initialize({ Opened.Setup }).join() }
        val receiving = mutableStateOf(ReceiveWizardIntent())
        val navigation = IdentityNavigation(null, actions, receiving) {}
        val original = checkNotNull(actions.state.value.shown)
        val creates = AtomicInteger()
        val create = {
            creates.incrementAndGet()
            ready()
        }
        assertFalse(receiving.value.active)
        val created = checkNotNull(navigation.createFromSetup(original, create, receive = true))
        assertNull(navigation.createFromSetup(original, create, receive = true))
        withTimeout(5_000) { created.join() }
        val oldIntent = receiving.value
        assertTrue(oldIntent.matches(actions.state.value.shown?.opened))
        navigation.leaveReceiving(oldIntent)
        withTimeout(5_000) { checkNotNull(actions.perform({ Opened.Setup })).join() }
        assertNull(navigation.createFromSetup(original, create, receive = true))
        val current = checkNotNull(actions.state.value.shown)
        withTimeout(5_000) { checkNotNull(navigation.createFromSetup(current, create, receive = true)).join() }
        assertNotSame(oldIntent, receiving.value)
        val successor = receiving.value
        navigation.leaveReceiving(oldIntent)
        assertSame(successor, receiving.value)
        assertEquals(2, creates.get())
    }

    @Test
    fun ordinaryCreationDoesNotGrantWizardCleanupIntent() = runBlocking {
        val actions = IdentityActions(this) { Shown(it()) }
        withTimeout(5_000) { actions.initialize({ Opened.Setup }).join() }
        val receiving = mutableStateOf(ReceiveWizardIntent())
        val navigation = IdentityNavigation(null, actions, receiving) {}
        val shown = checkNotNull(actions.state.value.shown)
        withTimeout(5_000) { checkNotNull(navigation.createFromSetup(shown, ::ready, receive = false)).join() }
        assertFalse(receiving.value.active)
    }

    @Test
    fun restoredIntentOnlyRoutesTheMatchingIdentityAndAnExplicitDestinationRetiresIt() = runBlocking {
        val actions = IdentityActions(this) { Shown(it()) }
        val receiving = mutableStateOf(ReceiveWizardIntent("wizard", "peer"))
        assertTrue(receiving.value.matches(ready()))
        assertFalse(receiving.value.matches(Opened.Setup))
        val other = Opened.Ready(Slot("other", File("presentation-only"), SlotState.READY), "peer")
        assertFalse(receiving.value.matches(other))
        IdentityNavigation(StartTarget.SETTINGS, actions, receiving) {}
        assertFalse(receiving.value.active)
    }
}
