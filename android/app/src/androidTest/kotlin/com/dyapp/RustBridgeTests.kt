package com.dyapp

import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertEquals
import org.junit.Test

// Empty-app lifecycle smoke coverage. Rust FFI is outside this milestone.
class ActivityLifecycleTests {
    @Test
    fun activityCanBeClosed() {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        assertEquals(Lifecycle.State.RESUMED, scenario.state)
        scenario.close()
        assertEquals(Lifecycle.State.DESTROYED, scenario.state)
    }
}
