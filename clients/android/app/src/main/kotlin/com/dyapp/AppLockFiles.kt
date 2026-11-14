package com.dyapp

import uniffi.dyapp.AppLockStorageException
import uniffi.dyapp.KeyStore
import uniffi.dyapp.MobileAppLockEntry
import uniffi.dyapp.MobileAppLockStorage
import uniffi.dyapp.WrapException
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.BasicFileAttributes

/** Protected local IO only: framing, mode precedence and transitions belong to the Rust core. */
internal class AppLockFiles(
    private val root: File,
    private val keys: KeyStore,
    private val syncDirectory: (File) -> Unit = ::syncLockDirectory,
) : MobileAppLockStorage {
    override fun readEntry(entry: MobileAppLockEntry): ByteArray? = protectedLockIO {
        checkRoot()
        val file = file(entry)
        if (attributes(file) == null) {
            null
        } else {
            if (entry == MobileAppLockEntry.RECORD) keys.unwrap(readCiphertext(file)) else byteArrayOf()
        }
    }

    override fun writeEntry(entry: MobileAppLockEntry, data: ByteArray) = protectedLockIO {
        val wrapped = try {
            checkRoot()
            if (entry == MobileAppLockEntry.RECORD) {
                keys.wrap(data)
            } else {
                if (data.isNotEmpty()) throw IOException("Invalid app-lock marker")
                byteArrayOf()
            }
        } finally {
            data.fill(0)
        }
        if (wrapped.size > MAX_FILE_BYTES) throw IOException(INVALID_FILE)
        writeAtomic(file(entry), wrapped)
    }

    override fun removeEntry(entry: MobileAppLockEntry) = protectedLockIO {
        checkRoot()
        val file = file(entry)
        if (attributes(file) != null) {
            Files.delete(file.toPath())
            syncDirectory(root)
        }
    }

    private fun file(entry: MobileAppLockEntry) = File(
        root,
        when (entry) {
            MobileAppLockEntry.RECORD -> "app-lock"
            MobileAppLockEntry.SKIPPED -> AppLockStore.SKIPPED
            MobileAppLockEntry.BIOMETRIC -> AppLockStore.BIOMETRIC
        }
    )

    private fun checkRoot() {
        val attributes = Files.readAttributes(root.toPath(), BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
        if (!attributes.isDirectory) throw IOException("Invalid app-lock directory")
    }

    private fun attributes(file: File): BasicFileAttributes? {
        val attributes = try {
            Files.readAttributes(file.toPath(), BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
        } catch (_: NoSuchFileException) {
            return null
        }
        if (!attributes.isRegularFile || attributes.size() > MAX_FILE_BYTES) throw IOException(INVALID_FILE)
        return attributes
    }

    private fun readCiphertext(file: File): ByteArray {
        val buffer = ByteBuffer.allocate(MAX_FILE_BYTES + 1)
        FileChannel.open(file.toPath(), StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS).use { channel ->
            while (buffer.hasRemaining()) {
                if (channel.read(buffer) < 0) break
            }
        }
        if (buffer.position() > MAX_FILE_BYTES) throw IOException(INVALID_FILE)
        return buffer.array().copyOf(buffer.position())
    }

    private fun writeAtomic(file: File, bytes: ByteArray) {
        val temp = File(root, file.name + ".tmp")
        FileChannel.open(
            temp.toPath(),
            StandardOpenOption.CREATE,
            StandardOpenOption.WRITE,
            StandardOpenOption.TRUNCATE_EXISTING,
            LinkOption.NOFOLLOW_LINKS,
        ).use { channel ->
            val buffer = ByteBuffer.wrap(bytes)
            while (buffer.hasRemaining()) channel.write(buffer)
            channel.force(true)
        }
        Files.move(temp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        syncDirectory(root)
    }

    private companion object {
        const val MAX_FILE_BYTES = 256
        const val INVALID_FILE = "Invalid app-lock file"
    }
}

private fun syncLockDirectory(root: File) {
    FileChannel.open(root.toPath(), StandardOpenOption.READ).use { it.force(true) }
}

private fun <T> protectedLockIO(action: () -> T): T = try {
    action()
} catch (error: IOException) {
    throw lockStorageFailure(error, "Protected app-lock IO failed")
} catch (error: WrapException) {
    throw lockStorageFailure(error, "Protected app-lock key unavailable")
} catch (error: SecurityException) {
    throw lockStorageFailure(error, "Protected app-lock access refused")
}

private fun lockStorageFailure(error: Exception, fallback: String) =
    AppLockStorageException.Failed(error.message ?: fallback).apply { initCause(error) }
