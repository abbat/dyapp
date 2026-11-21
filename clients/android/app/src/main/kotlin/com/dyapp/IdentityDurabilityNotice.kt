package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics

@Composable
private fun IdentityDurabilityBody() {
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(
            stringResource(R.string.identity_persistence_notice),
            Modifier.testTag("identity-durability-notice").semantics { liveRegion = LiveRegionMode.Polite }
        )
    }
}

/** Acknowledges an already committed result; the native diagnostic is never a retry action. */
@Composable
internal fun IdentityDurabilityNotice(shown: Shown) {
    var visible by remember(shown) { mutableStateOf(shown.durabilityDetail != null) }
    if (!visible) return
    val acknowledge = { visible = false }
    AlertDialog(
        onDismissRequest = acknowledge,
        modifier = Modifier.testTag("identity-durability-dialog"),
        title = {
            Text(
                stringResource(R.string.identity_persistence_title),
                Modifier.testTag("identity-durability-title")
            )
        },
        text = { IdentityDurabilityBody() },
        confirmButton = {
            TextButton(
                acknowledge,
                Modifier.testTag("identity-durability-ack").applicationClick(acknowledge)
            ) {
                Text(stringResource(R.string.identity_persistence_ack))
            }
        }
    )
}
