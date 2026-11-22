package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
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
