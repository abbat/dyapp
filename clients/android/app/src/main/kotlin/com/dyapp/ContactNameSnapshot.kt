package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import uniffi.dyapp.ContactNameView

/** A refresh may keep this owner's names; a new owner or peer selection must start empty. */
@Composable
internal fun rememberContactNameSnapshot(
    owner: Any,
    revision: Int?,
    read: suspend () -> List<ContactNameView>
): Map<String, ContactNameView> = key(owner) {
    val names by produceState(emptyMap<String, ContactNameView>(), revision) {
        val result = runCatching { read() }
        currentCoroutineContext().ensureActive()
        result.onSuccess { value = it.associateBy { name -> name.peerId } }
    }
    names
}
