package com.dyapp

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import java.util.Locale

internal const val APPEARANCE_STORE = "appearance"
private const val THEME = "theme"
private const val LANGUAGE = "language"

internal enum class ThemeChoice(val label: Int) {
    SYSTEM(R.string.settings_follow_system),
    LIGHT(R.string.settings_theme_light),
    DARK(R.string.settings_theme_dark),
}

/** Named by the language tag in lowercase, except [SYSTEM]. */
internal enum class LanguageChoice(val label: Int) {
    SYSTEM(R.string.settings_follow_system),
    EN(R.string.settings_language_en),
    RU(R.string.settings_language_ru),
}

// Compose state, so picking a theme recolours the app without a relaunch.
private val chosenTheme = mutableStateOf(ThemeChoice.SYSTEM)

// The section a language relaunch returns to.
private var relaunchTarget: StartTarget? = null

private val Context.appearance get() = getSharedPreferences(APPEARANCE_STORE, Context.MODE_PRIVATE)

private fun Context.savedLanguage() =
    LanguageChoice.values().find { it.name == appearance.getString(LANGUAGE, null) } ?: LanguageChoice.SYSTEM

/** Reads the saved theme into the state the app colours follow. */
internal fun Context.loadTheme() {
    chosenTheme.value =
        ThemeChoice.values().find { it.name == appearance.getString(THEME, null) } ?: ThemeChoice.SYSTEM
}

@Composable
internal fun chosenDarkTheme() = when (chosenTheme.value) {
    ThemeChoice.SYSTEM -> isSystemInDarkTheme()
    ThemeChoice.LIGHT -> false
    ThemeChoice.DARK -> true
}

/** This context in the saved language; following the system leaves it as it is. */
internal fun Context.inChosenLanguage(): Context {
    val language = savedLanguage()
    if (language == LanguageChoice.SYSTEM) return this
    val config = Configuration(resources.configuration)
    config.setLocale(Locale.forLanguageTag(language.name.lowercase()))
    return createConfigurationContext(config)
}

/** The section to open after a language relaunch, once. */
internal fun takeRelaunchTarget() = relaunchTarget.also { relaunchTarget = null }

// Resources are fixed when the activity attaches, so a new language needs a new activity.
private fun Activity.pickLanguage(language: LanguageChoice) {
    appearance.edit().putString(LANGUAGE, language.name).apply()
    relaunchTarget = StartTarget.SETTINGS
    recreate()
}

/** The Settings card with the theme and the language; both apply to the whole app. */
@Composable
internal fun AppearanceCard() {
    // The view keeps the activity even where a test overrides LocalContext.
    val activity = LocalView.current.context as Activity
    val language = activity.savedLanguage()
    FormCard {
        CardTitle(R.string.settings_appearance)
        Column(Modifier.selectableGroup().testTag(THEME)) {
            Text(stringResource(R.string.settings_theme))
            ThemeChoice.values().forEach { choice ->
                ChoiceRow("theme-${choice.name.lowercase()}", choice.label, choice == chosenTheme.value) {
                    activity.appearance.edit().putString(THEME, choice.name).apply()
                    chosenTheme.value = choice
                }
            }
        }
        Column(Modifier.selectableGroup().testTag(LANGUAGE)) {
            Text(stringResource(R.string.settings_language))
            LanguageChoice.values().forEach { choice ->
                ChoiceRow("language-${choice.name.lowercase()}", choice.label, choice == language) {
                    if (choice != language) activity.pickLanguage(choice)
                }
            }
        }
    }
}

@Composable
private fun ChoiceRow(tag: String, label: Int, selected: Boolean, onPick: () -> Unit) = Row(
    Modifier.fillMaxWidth().testTag(tag).selectable(selected, role = Role.RadioButton, onClick = onPick)
        .padding(vertical = 8.dp),
    Arrangement.spacedBy(12.dp),
    Alignment.CenterVertically
) {
    RadioButton(selected, onClick = null)
    Text(stringResource(label))
}
