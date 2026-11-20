package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextReplacement
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.CoreException
import uniffi.dyapp.appPinInput
import uniffi.dyapp.appPinReady
import uniffi.dyapp.hashAppPin

/** Actual secure widget and native policy; no KDF, store, network or authentication fixture. */
class AppPinInputTests {
    @get:Rule
    val compose = createComposeRule()
    private val entered = mutableStateOf("")

    private fun show() = compose.setContent {
        Column {
            PinField(entered, R.string.lock_pin, "core-pin", true)
            Text(if (appPinReady(entered.value)) "ready" else "waiting", Modifier.testTag("readiness"))
        }
    }

    private fun replace(input: String, expected: String, readiness: String) {
        compose.onNodeWithTag("core-pin").performTextReplacement(input)
        compose.runOnIdle { assertEquals(expected, entered.value) }
        compose.onNodeWithTag("readiness").assertTextEquals(readiness)
    }

    @Test
    fun secureFieldUsesCoreAsciiFilteringCapAndReadiness() {
        show()
        compose.onNodeWithTag("core-pin").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password))
        replace("\uFF11\uFF12a12345", "12345", "waiting")
        replace("\uFF11\uFF12a123456", "123456", "ready")
        replace("9".repeat(65), "9".repeat(64), "ready")
        compose.runOnIdle { entered.value = "" }
        compose.onNodeWithTag("core-pin").assertTextEquals("")
    }

    @Test
    fun confirmationRetainsExistingErrorPriorityAndLocalizedResources() {
        assertEquals(R.string.lock_pin_short, pinRejection("12345", "54321"))
        assertEquals(R.string.lock_pin_mismatch, pinRejection("123456", "654321"))
        assertNull(pinRejection("123456", "123456"))
    }

    @Test
    fun assistanceCannotNormalizeRawAuthenticationThroughJni() {
        val input = "1a23456"
        assertEquals("123456", appPinInput(input))
        val failure = assertThrows(CoreException::class.java) { hashAppPin(input) }
        assertFalse(failure.toString().contains(input))
    }
}
