package com.dyapp

import android.Manifest
import android.annotation.SuppressLint
import android.content.pm.PackageManager
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow
import uniffi.dyapp.setTestNodeIdPowBits
import java.io.File

private const val LAN_REFUSAL_BUDGET_MS = 5_000u

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
        if (phase == "legacy") {
            NativeSlotFixture().use { fixture ->
                fixture.slots.create().connectionChecker().use { checker ->
                    checkConnection(checker, NetworkSettings(listOf(node), ""), allowed)
                }
            }
        } else {
            val publicNode = checkNotNull(arguments.getString("localNetworkPublicNode")) {
                "Missing isolated non-LAN node fixture"
            }
            restoredPhase(phase, node, publicNode, allowed)
        }
    }

    private fun restoredPhase(phase: String, node: String, publicNode: String, allowed: Boolean) {
        val files = InstrumentationRegistry.getInstrumentation().targetContext.noBackupFilesDir
        val fixture = WorkerPeerFixture(
            File(files, "lan-permission-slots"),
            File(files, "lan-permission-owner.json")
        )
        val progress = PeerCheckpoint(File(files, "lan-permission-phase.json"))
        val settings = NetworkSettings(listOf(node), "")
        val target = if (phase == "granted") {
            prepareActive(fixture, settings)
        } else {
            val previous = progress.read()
            requirePreviousPhase(previous, phase, node, publicNode)
            val ready = fixture.reopen()
            val restored = checkNotNull(RustBridge.loadNetwork(ready, NetworkFile.ACTIVE))
            assertEquals("Saved active LAN settings changed", settings, restored)
            val peer = previous.getString("nodePeer")
            if (phase != "cleanup") {
                val row = ready.connectionChecker().use { checkConnection(it, restored, allowed) }
                if (allowed) assertEquals(peer, (row as SeedRow.Connected).peerId)
            }
            ready to peer
        }
        val (ready, peer) = target
        checkNonLan(ready, publicNode, peer)
        assertEquals(settings, RustBridge.loadNetwork(ready, NetworkFile.ACTIVE))
        if (phase == "cleanup") fixture.cleanup()
        progress.write(phaseReceipt(phase, node, publicNode, target))
    }

    private fun checkNonLan(ready: Opened.Ready, node: String, peer: String) {
        // This reserved destination tests the non-LAN policy path, not Internet reachability.
        ready.connectionChecker().use { checker ->
            val row = checkConnection(checker, NetworkSettings(listOf("$node/$peer"), ""), allowed = true)
            assertEquals("Non-LAN route reached a different node", peer, (row as SeedRow.Connected).peerId)
        }
    }

    private fun phaseReceipt(
        phase: String,
        node: String,
        publicNode: String,
        target: Pair<Opened.Ready, String>
    ): JSONObject {
        val (ready, peer) = target
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return JSONObject()
            .put("phase", phase)
            .put("node", node)
            .put("publicNode", publicNode)
            .put("nodePeer", peer)
            .put("owner", ready.peerId)
            .put("process", workerProcessMarker())
            .put("sdk", Build.VERSION.SDK_INT)
            .put("target", context.applicationInfo.targetSdkVersion)
    }

    private fun prepareActive(fixture: WorkerPeerFixture, settings: NetworkSettings): Pair<Opened.Ready, String> {
        val ready = fixture.prepare()
        val peer = ready.connectionChecker().use { checker ->
            val row = checkConnection(checker, settings, allowed = true) as SeedRow.Connected
            checker.activate()
            row.peerId
        }
        assertEquals(settings, RustBridge.loadNetwork(ready, NetworkFile.ACTIVE))
        return ready to peer
    }

    private fun requirePreviousPhase(previous: JSONObject, phase: String, node: String, publicNode: String) {
        val expected = when (phase) {
            "denied" -> "granted"
            "regranted" -> "denied"
            "revoked" -> "regranted"
            "cleanup" -> "revoked"
            else -> error("Unknown restored LAN phase: $phase")
        }
        assertEquals("Out-of-order permission transition", expected, previous.getString("phase"))
        assertEquals("LAN fixture changed between processes", node, previous.getString("node"))
        assertEquals("Non-LAN fixture changed between processes", publicNode, previous.getString("publicNode"))
        assertFalse("Permission phases reused a process", workerProcessMarker() == previous.getString("process"))
    }

    // The permission is read only after the legacy lane returns, so only API 37 reaches it.
    @SuppressLint("InlinedApi")
    private fun expectedAccess(phase: String): Boolean {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val required = localNetworkPermissionRequired(Build.VERSION.SDK_INT, context.applicationInfo.targetSdkVersion)
        if (phase == "legacy") {
            assertFalse("Legacy lane must not require a new permission", required)
            return true
        }
        assertTrue("Modern lane must exercise actual API37 enforcement", required)
        val allowed = when (phase) {
            "granted", "regranted", "cleanup" -> true
            "denied", "revoked" -> false
            else -> error("Unknown permission phase: $phase")
        }
        val expected = if (allowed) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED
        assertEquals(expected, context.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK))
        return allowed
    }

    private fun checkConnection(checker: ConnectionCheck, settings: NetworkSettings, allowed: Boolean): SeedRow {
        var rows = emptyList<SeedRow>()
        if (allowed) {
            try {
                PeerTransition("native permission fixture ${settings.seeds.single()} readiness").awaitSignal(
                    ready = { rows.singleOrNull() is SeedRow.Connected },
                    advance = { remaining ->
                        rows = checker.run(settings, remaining.coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt())
                    }
                )
            } catch (failure: IllegalStateException) {
                throw AssertionError("${failure.message}; last native connection result: $rows", failure)
            }
        } else {
            rows = checker.run(settings, LAN_REFUSAL_BUDGET_MS)
        }
        assertEquals(1, rows.size)
        if (allowed) {
            assertTrue(
                rows.single() is SeedRow.Connected
            )
        } else {
            assertTrue(rows.single() is SeedRow.Unreachable)
        }
        return rows.single()
    }
}
