package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.unit.Density
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.profileInterestLimits
import java.util.Locale

/** Actual production fields and public rows, with platform locale and current font density. */
internal class InterestGeometryFixture(private val compose: ComposeContentTestRule) {
    val scale = mutableStateOf(1f)
    val limits = profileInterestLimits()
    val longTag = "live music ".repeat(limits.maxTagChars.toInt()).take(limits.maxTagChars.toInt()).trimEnd()
    val tags = listOf(longTag, "tea, coffee; music")
    private val typed = mutableStateOf(tags.joinToString("\n"))
    private val search = SearchFilters(emptyList()).apply { interest = "Tea, Coffee" }

    fun show(language: String): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent { Content(context, configuration) }
        return context
    }

    @Composable
    private fun Content(context: Context, configuration: Configuration) {
        val density = LocalDensity.current
        CompositionLocalProvider(
            LocalContext provides context,
            LocalConfiguration provides configuration,
            LocalDensity provides Density(density.density, scale.value)
        ) {
            DYAppTheme {
                Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).testTag("interest-geometry")) {
                    ProfileInterestField(typed)
                    SearchInterestField(search)
                    ProfileRows(ProfileDraft(interests = tags), emptyList())
                }
            }
        }
    }
}
