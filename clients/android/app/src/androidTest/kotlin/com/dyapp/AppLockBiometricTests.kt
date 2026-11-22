package com.dyapp

import android.content.Context
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextInput
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

private class BiometricProbe {
    val finished = CompletableDeferred<Boolean>()
    val entered = MutableStateFlow(false)
    val attempts = AtomicInteger()
    val pinAttempts = AtomicInteger()
    val skipAttempts = AtomicInteger()
    val device = DeviceLock(true, Biometrics.READY) {
        attempts.incrementAndGet()
        entered.value = true
        finished.await()
    }
}

class AppLockBiometricTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishSetupBiometricSharesAdmissionAndReportsRefusal() = checkAdmission("en", false)

    @Test
    fun russianSetupBiometricSharesAdmissionAndReportsRefusal() = checkAdmission("ru", false)

    @Test
    fun settingsBiometricSharesPinAdmissionAndReportsRefusal() = checkAdmission("en", true)

    @Test
    fun retiredSetupCallbackCannotStartConfiguration() {
        var showing by mutableStateOf(true)
        val attempts = AtomicInteger()
        val device = DeviceLock(true, Biometrics.READY) {
            attempts.incrementAndGet()
            true
        }
        compose.setContent {
            MaterialTheme {
                if (showing) {
                    SetupScreen(device, { true }) { true }
                } else {
                    Text("replacement", Modifier.testTag("replacement"))
                }
            }
        }
        val click = compose.onNodeWithTag("lock-biometric").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { showing = false }
        compose.awaitNode(hasTestTag("replacement"))
        compose.runOnIdle { click() }
        compose.waitForIdle()
        assertEquals(0, attempts.get())
    }

    @Test
    fun retiredSettingsRouteCannotKeepAnOsResetCallback() {
        var mode by mutableStateOf(LockMode.RESET)
        val attempts = AtomicInteger()
        val device = DeviceLock(true, Biometrics.READY) {
            attempts.incrementAndGet()
            true
        }
        compose.setContent {
            MaterialTheme {
                LockSettingsScreen(mode, device, { _, _ ->
                    attempts.incrementAndGet()
                    PinAttempt.UNLOCKED
                }) {}
            }
        }
        compose.onNodeWithTag("pin-set").performTextInput("123456")
        compose.onNodeWithTag("pin-repeat").performTextInput("123456")
        val pin = callback("lock-change")
        val biometric = callback("lock-biometric")
        compose.runOnIdle { mode = LockMode.PIN }
        compose.awaitNode(hasTestTag("pin-current"))
        compose.runOnIdle {
            pin()
            biometric()
        }
        compose.waitForIdle()
        assertEquals(0, attempts.get())
    }

    private fun checkAdmission(language: String, settings: Boolean) {
        val probe = BiometricProbe()
        val context = show(language, settings, probe)
        fillPin()
        val pin = callback("lock-set")
        val skip = if (settings) null else callback("lock-skip")
        val biometric = callback("lock-biometric")
        try {
            compose.runOnIdle {
                biometric()
                biometric()
            }
            compose.awaitSnapshot(probe.entered, 5_000, "biometric admission") { it }
            compose.runOnIdle {
                pin()
                skip?.invoke()
            }
            assertPending(settings)
            assertEquals(1, probe.attempts.get())
            assertEquals(0, probe.pinAttempts.get())
            assertEquals(0, probe.skipAttempts.get())
            probe.finished.complete(false)
            compose.awaitNode(hasTestTag("lock-error")).assertTextEquals(context.getString(R.string.lock_failed))
            assertEquals(1, probe.attempts.get())
        } finally {
            probe.finished.complete(false)
        }
    }

    private fun fillPin() {
        compose.onNodeWithTag("pin-set").performTextInput("123456")
        compose.onNodeWithTag("pin-repeat").performTextInput("123456")
        compose.onNodeWithTag("lock-set").assertIsEnabled()
    }

    private fun callback(tag: String) = compose.onNodeWithTag(tag).fetchSemanticsNode().config[applicationClickKey]

    private fun assertPending(settings: Boolean) {
        compose.onNodeWithTag("lock-biometric").assertIsNotEnabled()
        compose.onNodeWithTag("lock-set").assertIsNotEnabled()
        compose.onNodeWithTag("pin-set").assertIsNotEnabled()
        compose.onNodeWithTag("lock-error").assertDoesNotExist()
        if (!settings) compose.onNodeWithTag("lock-skip").assertIsNotEnabled()
    }

    private fun show(language: String, settings: Boolean, probe: BiometricProbe): Context {
        val context = ownerVisualContext(language, 1f)
        compose.setContent {
            LocalizedTestContext(context, 1f) {
                MaterialTheme {
                    if (settings) {
                        LockSettingsScreen(LockMode.OFF, probe.device, { _, _ ->
                            probe.pinAttempts.incrementAndGet()
                            PinAttempt.UNLOCKED
                        }) {}
                    } else {
                        SetupScreen(probe.device, {
                            probe.skipAttempts.incrementAndGet()
                            true
                        }) {
                            probe.pinAttempts.incrementAndGet()
                            true
                        }
                    }
                }
            }
        }
        return context
    }
}
