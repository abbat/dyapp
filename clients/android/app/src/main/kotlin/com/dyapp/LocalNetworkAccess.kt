package com.dyapp

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf

private const val LAN_PERMISSION_PREFS = "local-network-permission"
private const val AUTO_REQUESTED = "auto-requested"

/** OS adapter only: Rust retains all socket/address and network-selection policy. */
internal class LocalNetworkAccess(private val activity: ComponentActivity) {
    private val requests = NetworkPermissionRequests {
        required() &&
            activity.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED
    }
    private val permission = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        requests.complete()
    }
    private val preferences by lazy { activity.getSharedPreferences(LAN_PERMISSION_PREFS, Context.MODE_PRIVATE) }

    private fun required() = localNetworkPermissionRequired(
        Build.VERSION.SDK_INT,
        activity.applicationInfo.targetSdkVersion
    )

    fun run(action: () -> Unit) = requests.run(
        { permission.launch(Manifest.permission.ACCESS_LOCAL_NETWORK) },
        action
    )

    /** Restore existing active LAN profiles after target/OS upgrade, without repeated automatic prompts. */
    fun firstUnlock() {
        if (!required() || preferences.getBoolean(AUTO_REQUESTED, false)) return
        preferences.edit().putBoolean(AUTO_REQUESTED, true).apply()
        run {}
    }

    fun close() = requests.clear()
}

/** Tests/previews without an activity retain their normal network action. The app supplies its OS adapter. */
internal val LocalNetworkRequest = staticCompositionLocalOf<(() -> Unit) -> Unit> { { action -> action() } }

@Composable
internal fun ProvideLocalNetworkAccess(access: LocalNetworkAccess, content: @Composable () -> Unit) =
    CompositionLocalProvider(LocalNetworkRequest provides access::run, content = content)
