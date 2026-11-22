package com.dyapp

import androidx.compose.foundation.clickable
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Rule
import org.junit.Test
import java.io.IOException

class AppLockStartupTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun blockedStorageLeavesAResponsiveLoadingSurfaceWithoutApplicationContent() {
        val gate = AppLockReadGate(StoredLock.OFF)
        try {
            val fixture = AppLockStartupFixture(compose)
            fixture.show(gate)
            compose.onNodeWithTag("app-lock-loading").assertIsDisplayed()
            compose.onNodeWithTag("initialized-content").assertDoesNotExist()
            compose.runOnIdle { assertEquals(emptyList<StoredLock>(), fixture.modes.value) }
            gate.close()
            assertEquals(listOf(StoredLock.OFF), fixture.initialized())
            compose.onNodeWithTag("app-lock-loading").assertDoesNotExist()
            compose.onNodeWithTag("initialized-content").assertIsDisplayed()
        } finally {
            gate.close()
        }
    }

    @Test
    fun unreadableStorageInitializesTheProtectedModeInsteadOfFreshSetup() {
        val modes = MutableStateFlow(emptyList<StoredLock>())
        val owner = Any()
        compose.setContent {
            AppLockStartup(owner, { throw IOException("unavailable") }, { modes.value += it }) {
                Text("ready")
            }
        }
        val observed = compose.awaitSnapshot(modes, 5_000, "failed app-lock read") { it.isNotEmpty() }
        assertEquals(listOf(StoredLock.PIN), observed)
    }

    @Test
    fun restoredPrivateContentKeepsItsSavedStateWithANewStartupOwner() {
        val owners = MutableStateFlow(emptyList<Any>())
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            val owner = remember { Any() }
            AppLockStartup(owner, { StoredLock.OFF }, { owners.value += owner }) {
                var dismissed by rememberSaveable { mutableStateOf(false) }
                Text(
                    dismissed.toString(),
                    Modifier.testTag("saved-dismissal").clickable { dismissed = true }
                )
            }
        }
        val first = compose.awaitSnapshot(owners, 5_000, "initial startup") { it.size == 1 }.single()
        compose.onNodeWithTag("saved-dismissal").performClick().assertTextEquals("true")
        restoration.emulateSavedInstanceStateRestore()
        val restored = compose.awaitSnapshot(owners, 5_000, "restored startup") { it.size == 2 }.last()
        assertNotSame(first, restored)
        compose.onNodeWithTag("saved-dismissal").assertTextEquals("true")
    }

    @Test
    fun aRetiredOwnersLateUnprotectedModeCannotInitializeItsReplacement() {
        val old = AppLockReadGate(StoredLock.OFF)
        val next = AppLockReadGate(StoredLock.PIN)
        try {
            val fixture = AppLockStartupFixture(compose)
            fixture.show(old)
            next.close()
            fixture.show(next)
            assertEquals(listOf(StoredLock.PIN), fixture.initialized())
            old.close()
            compose.awaitSnapshot(old.finished, 5_000, "retired app-lock read") { it }
            compose.waitForIdle()
            assertEquals(listOf(StoredLock.PIN), fixture.modes.value)
        } finally {
            old.close()
            next.close()
        }
    }
}
