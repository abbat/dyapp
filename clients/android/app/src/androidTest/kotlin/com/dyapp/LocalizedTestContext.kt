package com.dyapp

import android.content.Context
import android.content.MutableContextWrapper
import android.content.res.Configuration
import android.view.ContextThemeWrapper
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.activity.result.ActivityResultRegistryOwner
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCompositionContext
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.viewinterop.AndroidView

/** A real configured parent View also supplies the context of separate platform Dialog windows. */
@Composable
internal fun LocalizedTestContext(
    context: Context,
    fontScale: Float = LocalDensity.current.fontScale,
    content: @Composable () -> Unit,
) {
    val registry = checkNotNull(LocalActivityResultRegistryOwner.current)
    val windowContext = LocalView.current.context
    val configured = remember(context, fontScale, windowContext) {
        configuredWindowContext(windowContext, context, fontScale)
    }
    val currentContext by rememberUpdatedState(configured)
    val currentContent by rememberUpdatedState(content)
    val parent = rememberCompositionContext()
    AndroidView(
        factory = {
            ComposeView(MutableContextWrapper(configured)).apply {
                setParentCompositionContext(parent)
                setContent { LocalizedTestProviders(currentContext, registry, currentContent) }
            }
        },
        modifier = Modifier.fillMaxSize(),
        onRelease = { it.disposeComposition() },
        update = { view ->
            val wrapper = view.context as MutableContextWrapper
            if (wrapper.baseContext !== configured) {
                wrapper.baseContext = configured
                view.dispatchConfigurationChanged(configured.resources.configuration)
            }
        }
    )
}

private fun configuredWindowContext(window: Context, localized: Context, scale: Float): ContextThemeWrapper =
    ContextThemeWrapper(window, 0).apply {
        applyOverrideConfiguration(Configuration(localized.resources.configuration).apply { fontScale = scale })
    }

@Composable
private fun LocalizedTestProviders(
    context: Context,
    registry: ActivityResultRegistryOwner,
    content: @Composable () -> Unit
) {
    val configuration = context.resources.configuration
    CompositionLocalProvider(
        LocalContext provides context,
        LocalConfiguration provides configuration,
        LocalResources provides context.resources,
        LocalActivityResultRegistryOwner provides registry,
        content = content
    )
}
