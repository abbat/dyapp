package com.dyapp

import kotlinx.coroutines.Deferred
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.MobileLinkCodeStatus
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal class LinkApprovalFixture {
    val status = AtomicReference(MobileLinkCodeStatus.VALID)
    val preparing = AtomicReference<() -> Unit>({})
    val sending = AtomicReference<() -> Unit>({})
    val created = AtomicInteger()
    val sent = AtomicInteger()
    val closed = AtomicInteger()
    val inspected = AtomicReference<String>()
    val input = AtomicReference<String>()
    val normalized = "core-normalized-public-code"
    val session = LinkApprovalSession(
        { text ->
            inspected.set(text)
            status.get()
        },
        { text ->
            input.set(text)
            created.incrementAndGet()
            preparing.get().invoke()
            LinkApprovalCalls(
                normalized,
                {
                    sent.incrementAndGet()
                    sending.get().invoke()
                },
                { closed.incrementAndGet() }
            )
        }
    )

    suspend fun prepare(): LinkApprovalOpening {
        await(checkNotNull(session.prepare("original pasted input")))
        return checkNotNull(session.state.value.opening)
    }

    suspend fun close() {
        withTimeout(5_000) { session.retire().await().getOrThrow() }
    }

    suspend fun await(completion: Deferred<Unit>) {
        withTimeout(5_000) { completion.await() }
    }
}
