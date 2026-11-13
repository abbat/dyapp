package com.dyapp

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Runs against the same registry that receiveMail opens, without clearing earlier test identities. */
class WorkerPeerPhaseTests {
    @Test
    fun runRequestedPhase() {
        val phase = InstrumentationRegistry.getArguments().getString("workerPeerPhase")
        assumeTrue(phase != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fixture = WorkerMailPair(context)
        when (phase) {
            "prepare" -> assertActive(context, prepare(context, fixture))
            "reopen" -> assertActive(context, fixture.reopen())
            "queue" -> assertActive(context, fixture.queueOffline())
            "reopenQueued" -> assertActive(context, fixture.reopenQueued())
            "upgraded" -> assertActive(context, upgraded(context, fixture))
            "catchUpScheduled" -> assertActive(context, fixture.catchUpScheduled())
            "cleanup" -> fixture.cleanup()
            else -> error("Unknown worker peer phase: $phase")
        }
    }

    private fun prepare(context: Context, fixture: WorkerMailPair): Opened.Ready {
        workerTestPolicy()
        versionCheckpoint(context).write(JSONObject().put("version", installedVersion(context)))
        return fixture.prepare()
    }

    private fun upgraded(context: Context, fixture: WorkerMailPair): Opened.Ready {
        val before = versionCheckpoint(context).read().getLong("version")
        check(installedVersion(context) > before) { "Installed APK version did not increase" }
        return fixture.reopenQueued()
    }

    private fun versionCheckpoint(context: Context) =
        PeerCheckpoint(File(context.filesDir, "worker-app-version.json"))

    private fun installedVersion(context: Context): Long =
        context.packageManager.getPackageInfo(context.packageName, 0).longVersionCode

    private fun assertActive(context: Context, ready: Opened.Ready) {
        val active = RustBridge.slots(context).open() as Opened.Ready
        assertEquals(ready.slot.id, active.slot.id)
        assertEquals(ready.peerId, active.peerId)
        assertEquals(ready.peerId, RustBridge.withKey(active) { _, key -> key.peerId() })
    }
}
