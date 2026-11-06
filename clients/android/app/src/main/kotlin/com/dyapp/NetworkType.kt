package com.dyapp

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import uniffi.dyapp.setNetworkMetered

/** Process-wide reporting also covers background mail, without retaining an Activity. */
internal object NetworkType {
    var version by mutableIntStateOf(0)
        private set
    private var metered = true
    private var testOverride: Boolean? = null

    @Synchronized
    internal fun overrideForTest(value: Boolean?) {
        testOverride = value
        setNetworkMetered(value ?: metered)
        version += 1
    }

    @Synchronized
    private fun report(value: Boolean) {
        metered = value
        setNetworkMetered(testOverride ?: value)
        version += 1
    }

    fun start(context: Context) {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        report(isMetered(manager.getNetworkCapabilities(manager.activeNetwork)))
        // Initial read and callbacks run on the main thread, so a stale read cannot overwrite a callback.
        manager.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                report(isMetered(capabilities))
            }

            override fun onLost(network: Network) {
                report(true)
            }
        }, Handler(Looper.getMainLooper()))
    }
}

internal fun isMetered(capabilities: NetworkCapabilities?) =
    capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) != true
