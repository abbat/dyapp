package com.dyapp

import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

internal class ProfileLoadTests : ProfileLoadFixture() {
    @Test
    fun changingKnownPeersReplacesTheShownProfileInTheSameComposition() {
        val selected = selection()
        val other = found.copy(peerId = "peer-other")
        showLoad(selected) { error("known profiles need no fetch") }
        compose.onNodeWithText(found.peerId).assertExists()
        compose.runOnIdle {
            selected.peer = other.peerId
            selected.known = other
        }
        compose.onNodeWithText(other.peerId).assertExists()
        compose.onNodeWithText(found.peerId).assertDoesNotExist()
        compose.runOnIdle { assertSame(other, (latest!!.value as ProfileLoad.Shown).found) }
    }

    @Test
    fun aLatePeerResponseCannotReplaceTheNewKnownProfile() {
        val selected = selection().apply { known = null }
        val gate = FetchGate()
        val other = found.copy(peerId = "peer-other")
        try {
            showLoad(selected) { gate.fetch() }
            awaitStep(gate.entered)
            val oldState = compose.runOnIdle { latest!! }
            compose.runOnIdle {
                selected.peer = other.peerId
                selected.known = other
            }
            compose.onNodeWithText(other.peerId).assertExists()
            gate.released.complete(found)
            awaitStep(gate.finished)
            compose.onNodeWithText(other.peerId).assertExists()
            compose.runOnIdle { assertEquals(ProfileLoad.Loading, oldState.value) }
        } finally {
            gate.released.complete(null)
        }
    }

    @Test
    fun changingIdentityRejectsItsOldResponseEvenForTheSamePeer() {
        val selected = selection().apply { known = null }
        val firstOwner = selected.owner
        val fresh = found.copy(version = found.version + 1uL)
        val gate = FetchGate()
        try {
            showLoad(selected) { if (selected.owner === firstOwner) gate.fetch() else fresh }
            awaitStep(gate.entered)
            compose.runOnIdle { selected.owner = owner("profile-load-other-owner") }
            compose.onNodeWithText(found.peerId).assertExists()
            gate.released.complete(found)
            awaitStep(gate.finished)
            compose.runOnIdle { assertSame(fresh, (latest!!.value as ProfileLoad.Shown).found) }
        } finally {
            gate.released.complete(null)
        }
    }

    @Test
    fun aSupersededRetryCannotOverwriteTheCompletedRetry() {
        val selected = selection().apply { known = null }
        val old = FetchGate()
        val current = FetchGate()
        val fresh = found.copy(version = found.version + 1uL)
        try {
            showLoad(selected) { if (selected.attempt == 0) old.fetch() else current.fetch() }
            awaitStep(old.entered)
            compose.runOnIdle { selected.attempt += 1 }
            awaitStep(current.entered)
            current.released.complete(fresh)
            awaitStep(current.finished)
            old.released.complete(found)
            awaitStep(old.finished)
            compose.runOnIdle { assertSame(fresh, (latest!!.value as ProfileLoad.Shown).found) }
        } finally {
            old.released.complete(null)
            current.released.complete(null)
        }
    }

    @Test
    fun missingProfilesRetryButShownProfilesDoNotRefetch() {
        val selected = selection().apply { known = null }
        var calls = 0
        showLoad(selected) {
            calls += 1
            if (calls == 1) null else found
        }
        compose.onNodeWithText("missing").assertExists()
        compose.runOnIdle { selected.attempt += 1 }
        compose.onNodeWithText(found.peerId).assertExists()
        compose.runOnIdle { selected.attempt += 1 }
        compose.onNodeWithText(found.peerId).assertExists()
        compose.runOnIdle { assertEquals(2, calls) }
    }

    @Test
    fun aKnownSnapshotForAnotherPeerIsNeverShown() {
        val selected = selection().apply { known = found.copy(peerId = "wrong-peer") }
        var calls = 0
        showLoad(selected) {
            calls += 1
            found
        }
        compose.onNodeWithText(found.peerId).assertExists()
        compose.onNodeWithText("wrong-peer").assertDoesNotExist()
        compose.runOnIdle { assertEquals(1, calls) }
    }
}
