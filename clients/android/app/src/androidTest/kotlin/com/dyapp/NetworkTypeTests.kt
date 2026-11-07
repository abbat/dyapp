package com.dyapp

import android.net.NetworkCapabilities
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkTypeTests {
    @Test
    fun onlyExplicitlyUnmeteredNetworksAllowAutomaticDownloads() {
        assertTrue(isMetered(null))
        assertTrue(isMetered(intArrayOf()))
        assertTrue(isMetered(intArrayOf(NetworkCapabilities.NET_CAPABILITY_INTERNET)))
        assertFalse(isMetered(intArrayOf(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)))
        assertFalse(isMetered(intArrayOf(
            NetworkCapabilities.NET_CAPABILITY_INTERNET,
            NetworkCapabilities.NET_CAPABILITY_NOT_METERED,
        )))
    }
}
