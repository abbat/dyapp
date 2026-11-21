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
    private var observed = "No accessibility root observed"

    init {
        val info = automation.serviceInfo
        info.flags = info.flags or AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        automation.serviceInfo = info
        automation.setOnAccessibilityEventListener { events.offer(Unit) }
    }

    fun deny() = click("permission_deny_button")
    fun allow() = click("permission_allow_button")

    fun awaitRequest() = awaitReady("pending system permission request") {
        val root = controllerRoot()
        val controller = root?.packageName?.toString()
        root != null && controller != null && buttons(root, controller, "permission_allow_button").size == 1
    }

    private fun click(button: String) {
        awaitReady("system permission controller / $button") {
            val root = controllerRoot()
            val controller = root?.packageName?.toString()
            if (root == null || controller == null) {
                false
            } else {
                val nodes = buttons(root, controller, button)
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

    private fun buttons(root: AccessibilityNodeInfo, controller: String, button: String): List<AccessibilityNodeInfo> {
        val pending = ArrayDeque<AccessibilityNodeInfo>()
        val nodes = mutableListOf<AccessibilityNodeInfo>()
        pending.add(root)
        var visited = 0
        val identifiers = mutableListOf<String>()
        while (pending.isNotEmpty()) {
            check(++visited <= 256) { "Permission-controller tree exceeds 256 nodes" }
            val node = pending.removeFirst()
            val id = node.viewIdResourceName
            if (id != null) identifiers.add(id)
            // An OEM package can retain the resources of the original controller namespace.
            val matches = node.packageName?.toString() == controller && id?.substringAfter(":id/", "") == button
            if (matches && node.isEnabled && node.isVisibleToUser) {
                nodes.add(node)
            }
            repeat(node.childCount) { index -> node.getChild(index)?.let(pending::addLast) }
        }
        observed = "Controller $controller; resource IDs: ${identifiers.distinct()}"
        return nodes
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
        observed = "Active package=$name; application flags=${owner?.flags}"
        return root?.takeIf { owner != null && owner.flags and system != 0 }
    }

    private fun awaitReady(step: String, ready: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(PERMISSION_DIALOG_SECONDS)
        while (!ready()) {
            val remaining = deadline - System.nanoTime()
            check(remaining > 0 && events.poll(remaining, TimeUnit.NANOSECONDS) != null) {
                "Permission transition not ready: $step; $observed"
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
