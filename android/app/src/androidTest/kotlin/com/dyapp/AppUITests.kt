package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
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
        val ownsActiveWindow = {
            instrumentation.uiAutomation.rootInActiveWindow?.packageName?.toString() ==
                instrumentation.targetContext.packageName
        }
        // Window focus moves to the activity asynchronously, after Compose is already idle.
        runCatching { compose.waitUntil(timeoutMillis = 10_000) { ownsActiveWindow() } }
        check(ownsActiveWindow()) { "Application UI is obscured by another active window" }
        // The app's own window via PixelCopy: UiAutomation.takeScreenshot() returns null on the
        // software-rendered API 31 emulator.
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
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
