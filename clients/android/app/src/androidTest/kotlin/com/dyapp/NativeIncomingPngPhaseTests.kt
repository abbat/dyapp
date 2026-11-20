package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Requires the suite-owned loopback host client; the phone remains a normal client. */
class NativeIncomingPngPhaseTests {
    @Test
    fun realPngKnownAndUnknownGeometryRevalidateHostileCacheAndDecodeExactPixels() {
        val phase = InstrumentationRegistry.getArguments().getString("incomingPngPhase")
        assumeTrue(phase != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fixture = RetainedPngFixture(context)
        when (phase) {
            "prepare" -> fixture.prepare()
            "reopenOffline" -> fixture.reopenOffline()
            "cleanup" -> fixture.cleanup()
            else -> error("Unknown incoming PNG phase: $phase")
        }
    }
}
