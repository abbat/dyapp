package com.dyapp

import android.content.res.Configuration
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import java.util.Locale
import org.junit.Rule
import org.junit.Test

/** Practice D1: the start screen in the en-XA pseudo-locale (accented, about 30% longer). */
class PseudoLocaleTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun startScreenRendersRussianStrings() {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration)
        configuration.setLocale(Locale.forLanguageTag("ru"))
        val russian = base.createConfigurationContext(configuration)
        val ready = russian.getString(R.string.status_ready)
        check(ready == "\u0413\u043e\u0442\u043e\u0432\u043e") { "Russian translation missing: $ready" }
        compose.setContent {
            CompositionLocalProvider(
                LocalContext provides russian,
                LocalConfiguration provides configuration
            ) { MainScreen() }
        }
        compose.skipNetworkEntry()
        compose.onNodeWithText(russian.getString(R.string.app_name)).assertIsDisplayed()
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals(ready)
    }

    @Test
    fun startScreenRendersPseudoLocalizedStrings() {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration)
        configuration.setLocale(Locale.forLanguageTag("en-XA"))
        val pseudo = base.createConfigurationContext(configuration)
        val ready = pseudo.getString(R.string.status_ready)
        check(ready != base.getString(R.string.status_ready)) { "en-XA not generated: $ready" }
        compose.setContent {
            CompositionLocalProvider(
                LocalContext provides pseudo,
                LocalConfiguration provides configuration
            ) { MainScreen() }
        }
        compose.skipNetworkEntry()
        compose.onNodeWithText(pseudo.getString(R.string.app_name)).assertIsDisplayed()
        compose.onNodeWithTag("app-ready").assertIsDisplayed().assertTextEquals(ready)
    }
}
