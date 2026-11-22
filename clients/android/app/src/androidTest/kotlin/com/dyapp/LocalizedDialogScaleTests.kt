package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.activity.result.ActivityResultRegistryOwner
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.unit.Density
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test

class LocalizedDialogScaleTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aBareCompositionOverrideCannotProvideDialogScaleEvidence() {
        val base = InstrumentationRegistry.getInstrumentation().targetContext.resources.configuration.fontScale
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, base + 1f)) {
                ScaleDialog()
            }
        }
        compose.awaitNode(hasTestTag("scale-dialog"))
        val geometry = NativeVisualGeometry(compose, "scale-dialog")
        assertEquals(base, geometry.density(base).fontScale, 0f)
        assertThrows(IllegalStateException::class.java) { geometry.density(base + 1f) }
    }

    @Test
    fun equivalentContextWrappersRetainTheActualDialogHost() {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val revision = mutableIntStateOf(0)
        var observed: Context? = null
        compose.setContent {
            val localized = remember(revision.intValue) {
                base.createConfigurationContext(Configuration(base.resources.configuration))
            }
            LocalizedTestContext(localized, 1f) {
                val current = LocalContext.current
                SideEffect { observed = current }
                ScaleDialog()
            }
        }
        compose.awaitNode(hasTestTag("scale-dialog"))
        var original: Context? = null
        compose.runOnIdle { original = checkNotNull(observed) }
        compose.onNodeWithTag("scale-draft").performTextInput("retained")
        repeat(2) {
            compose.runOnIdle { revision.intValue++ }
            compose.awaitNode(hasTestTag("scale-dialog"))
            compose.runOnIdle { assertSame(original, observed) }
            compose.onNodeWithTag("scale-draft").assertTextEquals("retained")
        }
    }

    @Test
    fun actualDialogScaleChangesWithoutLosingDraftOrPickerRegistry() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val scale = mutableFloatStateOf(1f)
        var expectedRegistry: ActivityResultRegistryOwner? = null
        var observedRegistry: ActivityResultRegistryOwner? = null
        compose.setContent {
            val parentRegistry = LocalActivityResultRegistryOwner.current
            SideEffect { expectedRegistry = parentRegistry }
            LocalizedTestContext(context, scale.floatValue) {
                val childRegistry = LocalActivityResultRegistryOwner.current
                SideEffect { observedRegistry = childRegistry }
                ScaleDialog()
            }
        }
        compose.awaitNode(hasTestTag("scale-dialog"))
        compose.onNodeWithTag("scale-draft").performTextInput("retained")
        val geometry = NativeVisualGeometry(compose, "scale-dialog")
        for (font in listOf(1f, 2f, 1f)) {
            compose.runOnIdle { scale.floatValue = font }
            compose.awaitNode(hasTestTag("scale-dialog"))
            assertEquals(font, geometry.density(font).fontScale, 0f)
            compose.onNodeWithTag("scale-draft").assertTextEquals("retained")
            compose.runOnIdle {
                checkNotNull(expectedRegistry)
                assertSame(expectedRegistry, observedRegistry)
            }
        }
    }
}

@Composable
private fun ScaleDialog() {
    var draft by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = {},
        modifier = Modifier.testTag("scale-dialog"),
        title = { Text("Native scale witness") },
        text = { OutlinedTextField(draft, { draft = it }, Modifier.testTag("scale-draft")) },
        confirmButton = { TextButton({}) { Text("Confirm") } }
    )
}
