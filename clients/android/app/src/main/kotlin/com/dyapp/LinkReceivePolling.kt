package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.deviceLinkCodeStatus

private const val LINK_POLL_INTERVAL_MS = 3_000L
private const val LINK_PHASE_INTERVAL_MS = 100L

/** Scheduling is presentation only; core classifies expiry and owns every admission transition. */
@Composable
internal fun rememberLinkCodeStatus(
    opening: LinkReceiverOpening?,
    classify: suspend (String) -> MobileLinkCodeStatus?
): State<MobileLinkCodeStatus?> {
    val status = remember(opening) { mutableStateOf<MobileLinkCodeStatus?>(null) }
    LaunchedEffect(opening, classify) {
        val session = opening?.session ?: return@LaunchedEffect
        pollLinkReceiver(session, classify) { status.value = it }
    }
    return status
}

private suspend fun pollLinkReceiver(
    session: LinkReceiverSession,
    classify: suspend (String) -> MobileLinkCodeStatus?,
    show: (MobileLinkCodeStatus?) -> Unit
) {
    session.state.first { !it.busy }
    while (currentCoroutineContext().isActive) {
        val code = invitationToPoll(session.state.value) ?: break
        val status = classify(code)
        show(status)
        if (status != MobileLinkCodeStatus.VALID) {
            session.retire()
            break
        }
        if (!session.poll()) break
        observeLinkCompletion(session)
        delay(LINK_POLL_INTERVAL_MS)
    }
}

private fun invitationToPoll(snapshot: LinkReceiverSnapshot): String? {
    if (snapshot.unavailable || snapshot.phase != MobileLinkPhase.WAITING) return null
    return snapshot.display?.code
}

internal suspend fun readLinkCodeStatus(code: String) = withContext(Dispatchers.IO) {
    runCatching { deviceLinkCodeStatus(code) }.getOrNull()
}

private suspend fun observeLinkCompletion(session: LinkReceiverSession) {
    while (session.state.value.busy) {
        session.refreshPhase()
        delay(LINK_PHASE_INTERVAL_MS)
    }
}
