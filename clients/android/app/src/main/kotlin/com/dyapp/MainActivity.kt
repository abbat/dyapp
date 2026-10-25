package com.dyapp

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.dyapp.ui.theme.DYAppTheme

/** The single activity; hosts the Compose UI. */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { DYAppTheme { MainScreen() } }
    }
}

private val reasons = mapOf(
    "missing" to R.string.recovery_missing,
    "incomplete" to R.string.recovery_incomplete,
    "deleting" to R.string.recovery_deleting,
    "damaged" to R.string.recovery_damaged,
)

@Composable
private fun reasonText(reason: String) =
    stringResource(reasons[reason] ?: R.string.recovery_unavailable)

/**
 * The start screen: the active identity, the identity picker or a recovery state. `app-ready`
 * tags the status line UI tests wait for, `peer-id` the core's answer.
 */
@Composable
fun MainScreen() {
    val context = LocalContext.current.applicationContext
    val slots = remember { RustBridge.slots(context) }
    var shown by remember { mutableStateOf(RustBridge.show(slots, slots::open)) }
    val act = { action: () -> Opened -> shown = RustBridge.show(slots, action) }
    val opened = shown.opened
    val status = when (opened) {
        null -> R.string.status_error
        is Opened.Ready -> R.string.status_ready
        is Opened.Recovery -> R.string.status_recovery
        else -> R.string.status_pick
    }
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
        Text(stringResource(status), modifier = Modifier.testTag("app-ready"))
        when (opened) {
            null -> Text(stringResource(R.string.storage_error))
            is Opened.Ready -> {
                Text(stringResource(R.string.storage_peer_id))
                Text(opened.peerId, modifier = Modifier.testTag("peer-id"))
            }
            is Opened.Recovery -> {
                Text(reasonText(opened.reason), modifier = Modifier.testTag("recovery-reason"))
                opened.peerId?.let { Text(it, modifier = Modifier.testTag("recovery-peer")) }
                Button(onClick = { act(slots::open) }, Modifier.testTag("retry")) {
                    Text(stringResource(R.string.recovery_retry))
                }
            }
            else -> {
                for (entry in shown.entries) {
                    val (slot, label) = when (entry) {
                        is Opened.Ready -> entry.slot to entry.peerId
                        is Opened.Recovery -> entry.slot to reasonText(entry.reason)
                        else -> continue
                    }
                    val select = { slots.setActive(slot.id); slots.open() }
                    OutlinedButton(onClick = { act(select) }, Modifier.testTag("identity")) {
                        Text(label)
                    }
                }
                Button(onClick = { act(slots::create) }, Modifier.testTag("add-identity")) {
                    Text(stringResource(R.string.identity_add))
                }
            }
        }
        if (opened is Opened.Ready || opened is Opened.Recovery) {
            OutlinedButton(onClick = { act { Opened.Picker } }, Modifier.testTag("switch")) {
                Text(stringResource(R.string.identity_switch))
            }
        }
    }
}
