package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleCallback
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/** Observe recreation directly; ActivityScenario first requires RESUMED despite the open OS dialog. */
internal fun recreateWithPendingPermission(activity: MainActivity) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val monitor = ActivityLifecycleMonitorRegistry.getInstance()
    val destroyed = CompletableFuture<Unit>()
    val created = CompletableFuture<Unit>()
    val observer = ActivityLifecycleCallback { current, stage ->
        if (current === activity && stage == Stage.DESTROYED) destroyed.complete(Unit)
        if (current is MainActivity && current !== activity && stage == Stage.CREATED) created.complete(Unit)
    }
    instrumentation.runOnMainSync { monitor.addLifecycleCallback(observer) }
    try {
        instrumentation.runOnMainSync { activity.recreate() }
        destroyed.get(30, TimeUnit.SECONDS)
        created.get(30, TimeUnit.SECONDS)
    } finally {
        instrumentation.runOnMainSync { monitor.removeLifecycleCallback(observer) }
    }
}
