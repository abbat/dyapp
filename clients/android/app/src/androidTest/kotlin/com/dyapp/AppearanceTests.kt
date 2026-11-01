package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.core.view.WindowCompat
import androidx.test.platform.app.InstrumentationRegistry
import java.util.Locale
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

private const val RELAUNCH_MILLIS = 10_000L

class AppearanceTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val target = InstrumentationRegistry.getInstrumentation().targetContext

    // Choices persist on the emulator; the next test must meet the system theme and language.
    @After
    fun followTheSystemAgain() {
        target.getSharedPreferences(APPEARANCE_STORE, Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun openSettings() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
    }

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    private fun assertPicked(tag: String) = compose.onNodeWithTag(tag).performScrollTo().assertIsSelected()

    private fun lightBars() = compose.runOnIdle {
        val window = compose.activity.window
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars
    }

    private fun themeTitle(language: String): String {
        val config = Configuration(target.resources.configuration)
        config.setLocale(Locale.forLanguageTag(language))
        return target.createConfigurationContext(config).getString(R.string.settings_theme)
    }

    // The activity is replaced under the rule, so there may be no hierarchy to read for a moment.
    private fun waitForSettingsIn(language: String) = compose.waitUntil(RELAUNCH_MILLIS) {
        val title = themeTitle(language)
        val shown = runCatching { compose.onAllNodesWithText(title).fetchSemanticsNodes().isNotEmpty() }
        shown.getOrDefault(false)
    }

    @Test
    fun aPickedThemeAppliesAtOnceAndSurvivesARelaunch() {
        openSettings()
        click("theme-dark")
        check(!lightBars()) { "The dark theme kept light status bars" }
        compose.activityRule.scenario.recreate()
        openSettings()
        assertPicked("theme-dark")
        check(!lightBars()) { "The dark theme was lost on relaunch" }
        click("theme-light")
        check(lightBars()) { "The light theme kept dark status bars" }
    }

    @Test
    fun aPickedLanguageReturnsToSettingsAndSurvivesARelaunch() {
        openSettings()
        click("language-ru")
        waitForSettingsIn("ru")
        assertPicked("language-ru")
        compose.activityRule.scenario.recreate()
        openSettings()
        waitForSettingsIn("ru")
        assertPicked("language-ru")
        click("language-en")
        waitForSettingsIn("en")
        assertPicked("language-en")
    }
}
