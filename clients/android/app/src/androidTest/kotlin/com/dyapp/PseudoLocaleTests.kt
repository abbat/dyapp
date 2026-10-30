package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import java.util.Locale
import org.junit.Rule
import org.junit.Test

/** Practice D1: the start screen in the en-XA pseudo-locale (accented, about 30% longer). */
class PseudoLocaleTests {
    @get:Rule
    val compose = createComposeRule()

    private val base = InstrumentationRegistry.getInstrumentation().targetContext

    // The start screen in [language], past the automatic network draft.
    private fun render(language: String): Context {
        val configuration = Configuration(base.resources.configuration)
        configuration.setLocale(Locale.forLanguageTag(language))
        val localized = base.createConfigurationContext(configuration)
        compose.setContent {
            CompositionLocalProvider(
                LocalContext provides localized,
                LocalConfiguration provides configuration
            ) { MainScreen() }
        }
        compose.skipNetworkEntry()
        return localized
    }

    @Test
    fun startScreenRendersRussianStrings() {
        val russian = render("ru")
        val ready = russian.getString(R.string.nav_profile)
        check(ready == "\u041f\u0440\u043e\u0444\u0438\u043b\u044c") { "Russian translation missing: $ready" }
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals(ready)
    }

    @Test
    fun settingsRenderRussianStrings() {
        val russian = render("ru")
        val none = russian.getString(R.string.settings_network_none)
        check(none != base.getString(R.string.settings_network_none)) { "Russian translation missing: $none" }
        compose.onNodeWithTag("nav-settings").performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(russian.getString(R.string.nav_settings))
        compose.onNodeWithTag("settings-active").assertTextEquals(russian.getString(R.string.settings_active))
        compose.onNodeWithTag("settings-create").assertTextEquals(russian.getString(R.string.identity_add))
        // Other tests leave no active network.
        compose.waitUntil(CHECK_WAIT_MS) { compose.texts("settings-network-status") == listOf(none) }
    }

    @Test
    fun startScreenRendersPseudoLocalizedStrings() {
        val pseudo = render("en-XA")
        val ready = pseudo.getString(R.string.nav_profile)
        check(ready != base.getString(R.string.nav_profile)) { "en-XA not generated: $ready" }
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals(ready)
    }
}
