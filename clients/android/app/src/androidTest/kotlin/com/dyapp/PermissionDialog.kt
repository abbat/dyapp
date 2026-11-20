package com.dyapp

import android.accessibilityservice.AccessibilityServiceInfo
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.annotation.RequiresApi
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

private const val PERMISSION_DIALOG_SECONDS = 30L

/** API 37 system-dialog readiness; no translated text, colors or tap coordinates. */
@RequiresApi(37)
internal class PermissionDialog : AutoCloseable {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val automation = instrumentation.uiAutomation
    private val events = LinkedBlockingQueue<Unit>(1)
    private val originalFlags = automation.serviceInfo.flags

    init {
        val info = automation.serviceInfo
        info.flags = info.flags or AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        automation.serviceInfo = info
        automation.setOnAccessibilityEventListener { events.offer(Unit) }
    }

    fun deny() = click("permission_deny_button")
    fun allow() = click("permission_allow_button")

    private fun click(button: String) {
        awaitReady("system permission controller / $button") {
            val root = controllerRoot()
            val controller = root?.packageName?.toString()
            if (root == null || controller == null) {
                false
            } else {
                val nodes = root.findAccessibilityNodeInfosByViewId("$controller:id/$button")
                    .filter { it.isEnabled && it.isVisibleToUser }
                check(nodes.size <= 1) { "Ambiguous permission-controller button: $button" }
                nodes.singleOrNull()?.let {
                    check(it.performAction(AccessibilityNodeInfo.ACTION_CLICK)) { "Cannot click $button" }
                    true
                } ?: false
            }
        }
        awaitReady("application window") {
            automation.rootInActiveWindow?.packageName?.toString() == instrumentation.targetContext.packageName
        }
    }

    private fun controllerRoot(): AccessibilityNodeInfo? {
        val context = instrumentation.targetContext
        val root = automation.rootInActiveWindow
        val name = root?.packageName?.toString()?.takeUnless { it == context.packageName }
        val owner = name?.let {
            try {
                context.packageManager.getApplicationInfo(it, PackageManager.ApplicationInfoFlags.of(0))
            } catch (_: PackageManager.NameNotFoundException) {
                null
            }
        }
        val system = ApplicationInfo.FLAG_SYSTEM or ApplicationInfo.FLAG_UPDATED_SYSTEM_APP
        return root?.takeIf { owner != null && owner.flags and system != 0 }
    }

    private fun awaitReady(step: String, ready: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(PERMISSION_DIALOG_SECONDS)
        while (!ready()) {
            val remaining = deadline - System.nanoTime()
            check(remaining > 0 && events.poll(remaining, TimeUnit.NANOSECONDS) != null) {
                "Permission transition not ready: $step"
            }
        }
    }

    override fun close() {
        automation.setOnAccessibilityEventListener(null)
        val info = automation.serviceInfo
        info.flags = originalFlags
        automation.serviceInfo = info
    }
}
