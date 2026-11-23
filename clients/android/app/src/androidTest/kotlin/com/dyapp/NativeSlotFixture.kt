package com.dyapp

import android.content.Context
import android.content.ContextWrapper
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

    /** Preserve the caller's Activity/registry chain while replacing only owned application storage. */
    fun context(base: Context): Context = object : ContextWrapper(base) {
        override fun getApplicationContext(): Context = this
        override fun getNoBackupFilesDir(): File = root
    }

    override fun close() {
        slots.list().forEach { PlatformKeyStore.delete(identityAlias(it.id)) }
        root.deleteRecursively()
    }
}
