package com.dyapp

import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatSubmission

internal class ForwardStateFixture {
    val state = ChatForwardPresentation()
    val source = forwardSource()
    var reads = 0
    var sends = 0
    var failRead = false
    var failSend = false
    var outcome: ChatSubmission = ChatSubmission.Accepted(ByteArray(16))
    var sentSource: ChatForwardSource? = null
    var sentTarget: String? = null
    val calls = ChatForwardCalls(
        {
            reads += 1
            check(!failRead) { "controlled read refusal" }
            listOf("one", "two")
        },
        { selected, target ->
            sends += 1
            sentSource = selected
            sentTarget = target
            check(!failSend) { "controlled precommit refusal" }
            outcome
        }
    )

    fun ask(): ForwardSelection {
        state.ask(source)
        return checkNotNull(state.opening)
    }

    suspend fun ready(): ForwardSelection = ask().also { state.refresh(it, calls) }
}
