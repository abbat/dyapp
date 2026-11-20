package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.staticCompositionLocalOf

/** Presentation routes only; settings values and collection policy remain in the common core. */
internal enum class SettingsPane(val title: Int, val tag: String) {
    ROOT(R.string.nav_settings, "settings-root"),
    APPEARANCE(R.string.settings_appearance, "settings-appearance"),
    PRIVACY(R.string.settings_privacy_title, "settings-privacy"),
    NOTIFICATIONS(R.string.settings_notifications_title, "settings-notifications"),
    PHOTOS(R.string.settings_photos_title, "settings-photos"),
    ACCOUNT(R.string.settings_account_title, "settings-account"),
    ABOUT(R.string.settings_about_title, "settings-about"),
    IDENTITIES(R.string.settings_identities, "settings-identities-screen"),
    BLOCKED(R.string.settings_blocked, "settings-blocked"),
    PASSED(R.string.settings_passed, "settings-passed");

    fun parent(): SettingsPane = when (this) {
        IDENTITIES -> ACCOUNT
        BLOCKED, PASSED -> PRIVACY
        else -> ROOT
    }
}

internal val LocalSettingsPane = staticCompositionLocalOf<MutableState<SettingsPane>?> { null }

/** Keep only the route across the app-lock settings overlay, never native handles or values. */
internal class SettingsNavigation(private var owner: String = "", pane: SettingsPane = SettingsPane.ROOT) {
    private var route = mutableStateOf(pane)

    fun forOwner(current: String): MutableState<SettingsPane> {
        if (current != owner) {
            owner = current
            route = mutableStateOf(SettingsPane.ROOT)
        }
        return route
    }

    fun snapshot(): List<String> = listOf(owner, route.value.name)
}

internal val LocalSettingsNavigation = staticCompositionLocalOf<SettingsNavigation?> { null }

@Composable
internal fun rememberSettingsNavigation(): SettingsNavigation = rememberSaveable(
    saver = listSaver(
        save = { it.snapshot() },
        restore = { SettingsNavigation(it[0], SettingsPane.valueOf(it[1])) }
    )
) { SettingsNavigation() }

@Composable
internal fun rememberSettingsPane(owner: String): MutableState<SettingsPane> {
    val fallback = rememberSettingsNavigation()
    return (LocalSettingsNavigation.current ?: fallback).forOwner(owner)
}
