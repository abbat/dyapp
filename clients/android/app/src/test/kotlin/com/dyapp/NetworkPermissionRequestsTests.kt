package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkPermissionRequestsTests {
    @Test
    fun unsupportedSdkOrTargetNeverRequestsThePermission() {
        for ((sdk, target) in listOf(31 to 37, 36 to 37, 37 to 36)) {
            assertFalse(localNetworkPermissionRequired(sdk, target))
            var actions = 0
            val requests = NetworkPermissionRequests { localNetworkPermissionRequired(sdk, target) }
            requests.run({ error("Unsupported permission request") }) { actions++ }
            assertEquals(1, actions)
        }
        assertTrue(localNetworkPermissionRequired(37, 37))
    }

    @Test
    fun denialReleasesPublicNetworkingAndLaterGrantAndRevocationAreRechecked() {
        var granted = false
        var prompts = 0
        var actions = 0
        val requests = NetworkPermissionRequests { !granted }
        val launch: () -> Unit = { prompts++ }
        val action: () -> Unit = { actions++ }
        requests.run(launch, action)
        assertEquals(0, actions)
        requests.complete() // Denied: public Internet must still run.
        assertEquals(1, actions)
        granted = true
        requests.run(launch, action)
        assertEquals(2, actions)
        assertEquals(1, prompts)
        granted = false // Revocation must not reuse the earlier granted decision.
        requests.run(launch, action)
        assertEquals(2, prompts)
        assertEquals(2, actions)
        granted = true
        requests.complete()
        assertEquals(3, actions)
    }

    @Test
    fun onePromptCompletesEachQueuedActionOnceAndDestructionDropsOnlyPendingActions() {
        var prompts = 0
        var actions = 0
        val requests = NetworkPermissionRequests { true }
        repeat(2) { requests.run({ prompts++ }) { actions++ } }
        assertEquals(1, prompts)
        requests.complete()
        requests.complete()
        assertEquals(2, actions)
        requests.run({ prompts++ }) { actions++ }
        requests.clear()
        requests.complete()
        assertEquals(2, actions)
    }

    @Test
    fun disposedOwnerContinuationCannotStartWorkForTheNewSelection() {
        var originalPresent = true
        val started = mutableListOf<String>()
        val requests = NetworkPermissionRequests { true }
        requests.run({}) { if (originalPresent) started += "original" }
        originalPresent = false
        requests.complete()
        assertEquals(emptyList<String>(), started)
        requests.run({}) { started += "new" }
        requests.complete()
        assertEquals(listOf("new"), started)
    }
}
