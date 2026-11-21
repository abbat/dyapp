package com.dyapp

import android.content.Context
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.unit.Density

/** Keep localized resources and the real Activity's picker registry in the same test boundary. */
@Composable
internal fun LocalizedTestContext(
    context: Context,
    fontScale: Float = LocalDensity.current.fontScale,
    content: @Composable () -> Unit,
) {
    val registry = checkNotNull(LocalActivityResultRegistryOwner.current)
    val density = LocalDensity.current
    CompositionLocalProvider(
        LocalContext provides context,
        LocalConfiguration provides context.resources.configuration,
        LocalResources provides context.resources,
        LocalActivityResultRegistryOwner provides registry,
        LocalDensity provides Density(density.density, fontScale),
        content = content
    )
}
