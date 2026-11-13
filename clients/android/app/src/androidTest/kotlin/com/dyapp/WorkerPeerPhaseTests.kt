package com.dyapp

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Runs against the same registry that receiveMail opens, without clearing earlier test identities. */
class WorkerPeerPhaseTests {
    @Test
    fun runRequestedPhase() {
        val phase = InstrumentationRegistry.getArguments().getString("workerPeerPhase")
        assumeTrue(phase != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        if (phase == "prepare") workerTestPolicy()
        val fixture = WorkerMailPair(context)
        when (phase) {
            "prepare" -> assertActive(context, fixture.prepare())
            "reopen" -> assertActive(context, fixture.reopen())
            "queue" -> assertActive(context, fixture.queueOffline())
            "reopenQueued" -> assertActive(context, fixture.reopenQueued())
            "catchUpScheduled" -> assertActive(context, fixture.catchUpScheduled())
            "cleanup" -> fixture.cleanup()
            else -> error("Unknown worker peer phase: $phase")
        }
    }

    private fun assertActive(context: Context, ready: Opened.Ready) {
        val active = RustBridge.slots(context).open() as Opened.Ready
        assertEquals(ready.slot.id, active.slot.id)
        assertEquals(ready.peerId, active.peerId)
        assertEquals(ready.peerId, RustBridge.withKey(active) { _, key -> key.peerId() })
    }
}
