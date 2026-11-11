package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID

/** Own only aliases and files created under this isolated device fixture. */
internal class NativeSlotFixture : AutoCloseable {
    val root = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "native-slots-${UUID.randomUUID()}"
    )
    val slots = Slots(root)

    override fun close() {
        slots.list().forEach { PlatformKeyStore.delete(identityAlias(it.id)) }
        root.deleteRecursively()
    }
}
