package com.dyapp.ui.theme

import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat

/** App theme with platform dynamic colours on every supported Android version. */
@Composable
fun DYAppTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit
) {
    val context = LocalContext.current
    val colorScheme = if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)

    ThemeSystemBars(darkTheme)
    MaterialTheme(colorScheme = colorScheme, typography = Typography) {
        ThemeSurface(content)
    }
}

@Composable
private fun ThemeSystemBars(darkTheme: Boolean) {
    val view = LocalView.current
    val window = LocalActivity.current?.window
    if (!view.isInEditMode && window != null) {
        SideEffect {
            WindowCompat.enableEdgeToEdge(window)
            WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !darkTheme
                isAppearanceLightNavigationBars = !darkTheme
            }
        }
    }
}

@Composable
private fun ThemeSurface(content: @Composable () -> Unit) {
    val background = MaterialTheme.colorScheme.background
    Surface(
        modifier = Modifier.fillMaxSize().background(background).safeDrawingPadding(),
        color = background,
        content = content,
    )
}
