package com.dyapp

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow
import uniffi.dyapp.setTestNodeIdPowBits

private const val LAN_PROBE_WAIT_MS = 5_000u

/** Separate invocations survive the process termination caused by an external permission revoke. */
class LocalNetworkPermissionPhaseTests {
    @Test
    fun freshRustConnectionReflectsTheRequestedPlatformPermissionPhase() {
        val arguments = InstrumentationRegistry.getArguments()
        val phase = arguments.getString("localNetworkPermissionPhase")
        assumeTrue("Dedicated permission phase only", phase != null)
        val allowed = expectedAccess(checkNotNull(phase))
        val node = checkNotNull(arguments.getString("localNetworkNode")) { "Missing owned node fixture" }
        setTestNodeIdPowBits(8u)
        NativeSlotFixture().use { fixture ->
            fixture.slots.create().connectionChecker().use { checker ->
                checkConnection(checker, NetworkSettings(listOf(node), ""), allowed)
            }
        }
    }

    private fun expectedAccess(phase: String): Boolean {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val required = localNetworkPermissionRequired(Build.VERSION.SDK_INT, context.applicationInfo.targetSdkVersion)
        if (phase == "legacy") {
            assertFalse("Legacy lane must not require a new permission", required)
            return true
        }
        assertTrue("Modern lane must exercise actual API37 enforcement", required)
        val allowed = when (phase) {
            "granted", "regranted" -> true
            "denied", "revoked" -> false
            else -> error("Unknown permission phase: $phase")
        }
        val expected = if (allowed) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED
        assertEquals(expected, context.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK))
        return allowed
    }

    private fun checkConnection(checker: ConnectionCheck, settings: NetworkSettings, allowed: Boolean) {
        var rows = emptyList<SeedRow>()
        if (allowed) {
            PeerTransition("native LAN fixture readiness").await(
                ready = { rows.singleOrNull() is SeedRow.Connected },
                advance = { rows = checker.run(settings, LAN_PROBE_WAIT_MS) }
            )
        } else {
            rows = checker.run(settings, LAN_PROBE_WAIT_MS)
        }
        assertEquals(1, rows.size)
        if (allowed) {
            assertTrue(
                rows.single() is SeedRow.Connected
            )
        } else {
            assertTrue(rows.single() is SeedRow.Unreachable)
        }
    }
}
