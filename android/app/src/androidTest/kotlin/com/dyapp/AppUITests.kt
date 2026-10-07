package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Rule
import org.junit.Test

class AppUITests {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun launchesAndRendersReady() {
        compose.onNodeWithText("DYApp").assertIsDisplayed()
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals("Ready")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        check(instrumentation.uiAutomation.rootInActiveWindow?.packageName?.toString() ==
            instrumentation.targetContext.packageName) {
            "Application UI is obscured by another active window"
        }
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
            ?: error("Rendered application screenshot unavailable")
        File(instrumentation.targetContext.filesDir, "ui-ready.png").outputStream().use {
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
        }
    }

    @Test
    fun recreationKeepsReadyScreen() {
        compose.activityRule.scenario.recreate()
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals("Ready")
    }
}
