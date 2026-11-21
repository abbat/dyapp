package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember

/** The callbacks installed for one mounted confirmation, never a replacement opening. */
internal class ConfirmationActions(val cancel: () -> Unit, val confirm: () -> Unit, val enabled: Boolean)

private class ConfirmationOpening {
    private var active = true

    fun invoke(admitted: () -> Boolean, action: () -> Unit) {
        if (active && admitted()) {
            active = false
            action()
        }
    }

    fun retire() {
        active = false
    }
}

@Composable
internal fun rememberConfirmation(
    admitted: () -> Boolean,
    cancel: () -> Unit,
    confirm: () -> Unit,
): ConfirmationActions {
    val opening = remember { ConfirmationOpening() }
    DisposableEffect(opening) { onDispose { opening.retire() } }
    return ConfirmationActions(
        { opening.invoke(admitted, cancel) },
        { opening.invoke(admitted, confirm) },
        admitted()
    )
}
