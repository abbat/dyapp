package com.dyapp

import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics

/** Exact application callback for controlled completion/retirement tests, without a platform node lease. */
internal val applicationClickKey = SemanticsPropertyKey<() -> Unit>("ApplicationClick")

// Foundation's OnClick wrapper accesses its Modifier node before calling the application closure.
// Retaining that wrapper after disposal tests a detached node, not the application's owner guard.
internal fun Modifier.applicationClick(action: () -> Unit): Modifier = semantics {
    this[applicationClickKey] = action
}
