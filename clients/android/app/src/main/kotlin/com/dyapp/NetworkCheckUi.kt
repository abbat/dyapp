package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.CheckException
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
    else -> R.string.network_check_failed
}

@Composable
internal fun NetworkCheckResults(status: Int?, rows: List<Pair<String, SeedRow>>) {
    if (status != null) Text(stringResource(status), Modifier.testTag("network-check-status"))
    Column {
        rows.forEach { (seed, row) ->
            Text(seed + ": " + stringResource(rowStatus(row)), Modifier.testTag("network-row"))
        }
    }
}
