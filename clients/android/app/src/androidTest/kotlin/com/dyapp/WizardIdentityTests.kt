package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import java.io.File

class WizardIdentityTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theStepExplainsTheKeyAndAsksBeforeCreating() {
        var created = 0
        compose.setContent { WizardIdentity { created++ } }
        compose.onNodeWithTag("key-store").assertIsDisplayed()
        compose.onNodeWithTag("no-backup").assertIsDisplayed()
        check(created == 0) { "The step created an identity before it was asked to" }
        compose.onNodeWithTag("create-identity").performScrollTo().performClick()
        check(created == 1) { "Create identity did not run once" }
    }

    @Test
    fun aFreshInstallCreatesNothingAndAFailureIsNotASetup() {
        val cache = InstrumentationRegistry.getInstrumentation().targetContext.cacheDir
        val root = File(cache, "wizard-identity-test")
        try {
            val slots = Slots(root)
            check(RustBridge.show(slots, slots::open).opened == Opened.Setup) { "No identity step" }
            check(slots.list().isEmpty()) { "An identity appeared without confirmation" }
            val failed = RustBridge.show(slots) { error("expected by the test") }
            check(failed.opened == null) { "A failure did not reach the storage error" }
        } finally {
            root.deleteRecursively()
        }
    }
}
