package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.CheckException
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow

private fun rowStatus(row: SeedRow): Int = when (row) {
    is SeedRow.Connected -> R.string.network_row_connected
    SeedRow.InvalidAddress -> R.string.network_row_invalid_address
    SeedRow.InvalidPeerId -> R.string.network_row_invalid_peer
    SeedRow.Duplicate -> R.string.network_row_duplicate
    is SeedRow.Unreachable -> R.string.network_row_unreachable
    SeedRow.WrongPeer -> R.string.network_row_wrong_peer
    SeedRow.WrongNetwork -> R.string.network_row_wrong_network
}

internal fun checkSummary(rows: List<SeedRow>): Int = when {
    rows.any { it is SeedRow.Connected } -> R.string.network_connected
    rows.any { it is SeedRow.InvalidAddress || it is SeedRow.InvalidPeerId || it is SeedRow.Duplicate } ->
        R.string.network_invalid_address
    rows.any { it is SeedRow.WrongPeer || it is SeedRow.WrongNetwork } -> R.string.network_wrong
    else -> R.string.network_unreachable
}

internal fun checkFailure(error: Throwable): Int = when (error) {
    is CheckException.Empty -> R.string.network_empty
    is CheckException.InvalidNetwork -> R.string.network_invalid_name
    is CheckException.Cancelled -> R.string.network_cancelled
    else -> R.string.network_check_failed
}

private const val SEED_ROWS_TAG = "seed-rows"

/** What the check returned, one address per row; a null row is still being checked. */
@Composable
internal fun SeedRows(rows: List<Pair<String, SeedRow?>>) {
    if (rows.isEmpty()) return
    Column(Modifier.testTag(SEED_ROWS_TAG), Arrangement.spacedBy(8.dp)) {
        rows.forEach { (seed, row) ->
            Column {
                Text(seed, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                Text(
                    stringResource(if (row == null) R.string.network_row_checking else rowStatus(row)),
                    color = when (row) {
                        null -> MaterialTheme.colorScheme.onSurfaceVariant
                        is SeedRow.Connected -> MaterialTheme.colorScheme.primary
                        else -> MaterialTheme.colorScheme.error
                    },
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

/** Per address, the reason a dial failed, or the row's own status where nothing was dialled. */
@Composable
internal fun rowDetails(rows: List<Pair<String, SeedRow>>): String = rows.map { (seed, row) ->
    seed + ": " + ((row as? SeedRow.Unreachable)?.reason ?: stringResource(rowStatus(row)))
}.joinToString("\n")

/** The status line for this identity's active network, read again whenever [refresh] changes. */
@Composable
internal fun activeNetworkStatus(opened: Opened.Ready, refresh: Any? = null): String {
    val loaded by produceState<Result<NetworkSettings?>?>(null, opened.slot.id, refresh) {
        value = withContext(Dispatchers.IO) { runCatching { RustBridge.loadNetwork(opened, NetworkFile.ACTIVE) } }
    }
    val public = stringResource(R.string.settings_network_public)
    return loaded.let { result ->
        when {
            result == null -> stringResource(R.string.network_loading)
            result.isFailure -> stringResource(R.string.network_read_failed)
            else -> result.getOrNull()
                ?.let { stringResource(R.string.settings_network_active, it.name.ifEmpty { public }) }
                ?: stringResource(R.string.settings_network_none)
        }
    }
}

@Composable
internal fun NetworkCheckResults(status: Int?, rows: List<Pair<String, SeedRow>>) {
    if (status != null) Text(stringResource(status), Modifier.testTag("network-check-status"))
    Column(Modifier.testTag(SEED_ROWS_TAG)) {
        rows.forEach { (seed, row) -> Text(seed + ": " + stringResource(rowStatus(row))) }
    }
}
