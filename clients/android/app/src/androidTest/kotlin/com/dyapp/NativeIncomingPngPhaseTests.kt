package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Requires the suite-owned loopback host client; the phone remains a normal client. */
class NativeIncomingPngPhaseTests {
    @Test
    fun realPngKnownAndUnknownGeometryRevalidateHostileCacheAndDecodeExactPixels() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("incomingPngHost") == "1")
        TestPeer("incoming-png").use { receiver ->
            HostPhotoPeer(receiver.peerId).use { host ->
                IncomingPngChecks(receiver.session, receiver.directory).run(host)
            }
        }
    }
}
