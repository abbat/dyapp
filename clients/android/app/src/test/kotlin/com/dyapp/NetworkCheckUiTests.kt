package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.CheckException
import uniffi.dyapp.SeedRow

class NetworkCheckUiTests {
    @Test
    fun connectedRowWinsOverOtherFailures() {
        assertEquals(R.string.network_connected,
            checkSummary(listOf(SeedRow.InvalidAddress, SeedRow.Connected("peer"))))
    }

    @Test
    fun failuresKeepUsefulPriority() {
        assertEquals(R.string.network_invalid_address,
            checkSummary(listOf(SeedRow.WrongNetwork, SeedRow.InvalidPeerId)))
        assertEquals(R.string.network_wrong, checkSummary(listOf(SeedRow.WrongPeer)))
        assertEquals(R.string.network_unreachable,
            checkSummary(listOf(SeedRow.Unreachable("timeout"))))
        assertEquals(R.string.network_empty, checkFailure(CheckException.Empty()))
        assertEquals(R.string.network_invalid_name,
            checkFailure(CheckException.InvalidNetwork("invalid")))
    }
}
