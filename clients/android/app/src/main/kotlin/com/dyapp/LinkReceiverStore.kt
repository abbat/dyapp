package com.dyapp

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

/** Exact opening, including its original registry and identity; equal selectors are not ownership. */
internal class LinkReceiverOpening(val source: Opened.Ready, val session: LinkReceiverSession)

/** Keeps an unresolved native outcome reachable across activity disposal and profile changes. */
internal class LinkReceiverStore(private val create: (Opened.Ready, Boolean) -> LinkReceiverSession) {
    private val pending = MutableStateFlow<LinkReceiverOpening?>(null)
    val state = pending.asStateFlow()

    @Synchronized
    fun begin(source: Opened.Ready, wizard: Boolean): LinkReceiverOpening? {
        if (pending.value != null) return null
        val opening = LinkReceiverOpening(source, create(source, wizard))
        pending.value = opening
        check(opening.session.start())
        return opening
    }

    /** Only explicit disposition of this exact idle opening permits another invitation. */
    suspend fun release(opening: LinkReceiverOpening): Boolean = withContext(NonCancellable) {
        if (pending.value !== opening) return@withContext false
        if (!opening.session.close()) return@withContext false
        pending.compareAndSet(opening, null)
    }
}

/** No Activity or view scope owns the retained native worker. */
internal val pendingLinkReceiver = LinkReceiverStore { source, wizard ->
    LinkReceiverSession { linkReceiverCalls(source, wizard) }
}
