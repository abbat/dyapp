package com.dyapp

import android.net.NetworkCapabilities
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkTypeTests {
    @Test
    fun onlyExplicitlyUnmeteredNetworksAllowAutomaticDownloads() {
        assertTrue(isMetered(null))
        val capabilities = NetworkCapabilities()
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        assertTrue(isMetered(capabilities))
        capabilities.addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        assertFalse(isMetered(capabilities))
        capabilities.removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        assertTrue(isMetered(capabilities))
    }
}
