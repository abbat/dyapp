package com.dyapp

import android.app.KeyguardManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The single activity; checks the app lock before constructing the private Compose screen and
 * offers first-launch PIN setup. PIN work runs on a background dispatcher.
 */
class MainActivity : ComponentActivity() {
    private lateinit var appLock: AppLockLifecycle
    private val timer = Handler(Looper.getMainLooper())
    private val deadline = Runnable { refreshLock() }
    private val store by lazy { AppLockStore.onDevice(noBackupFilesDir) }
    private var locked by mutableStateOf(true)
    private var setup by mutableStateOf(false)
    private var settings by mutableStateOf(false)
    private var pinSet by mutableStateOf(false)
    private var deviceSecure by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // An unreadable verifier must fail closed, never start a new unlocked session.
        val (pinEnabled, setupDone) = runCatching { store.hasPin() to store.setupDone() }
            .getOrDefault(true to true)
        appLock = AppLockLifecycle(pinEnabled, SystemClock::elapsedRealtime)
        setup = !setupDone
        pinSet = pinEnabled
        obscureSnapshot(pinSet)
        refreshLock()
        setContent {
            DYAppTheme {
                when {
                    locked -> UnlockScreen(::attempt)
                    setup -> SetupScreen(deviceSecure, {
                        // A failed write only means the choice is offered again next launch.
                        runCatching { store.skip() }
                        setup = false
                    }) { setPin(null, it) == PinAttempt.UNLOCKED }
                    settings -> LockSettingsScreen(pinSet, deviceSecure, ::setPin) { settings = false }
                    else -> MainScreen { settings = true }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // The user may have just set a screen lock in system settings.
        deviceSecure = getSystemService(KeyguardManager::class.java).isDeviceSecure
        appLock.onForeground()
        refreshLock()
    }

    override fun onPause() {
        appLock.onBackground()
        refreshLock()
        super.onPause()
    }

    override fun onDestroy() {
        timer.removeCallbacks(deadline)
        super.onDestroy()
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        val wasLocked = locked
        appLock.onInteraction()
        refreshLock()
        return if (!wasLocked && locked) true else super.dispatchTouchEvent(event)
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val wasLocked = locked
        appLock.onInteraction()
        refreshLock()
        return if (!wasLocked && locked) true else super.dispatchKeyEvent(event)
    }

    private suspend fun attempt(pin: String): PinAttempt? {
        val result = withContext(Dispatchers.Default) { runCatching { store.attempt(pin) }.getOrNull() }
        // A PIN checked while the app went to the background does not unlock it.
        if (result == PinAttempt.UNLOCKED && !appLock.snapshotObscured) appLock.onUnlocked()
        refreshLock()
        return result
    }

    private suspend fun setPin(current: String?, newPin: String?): PinAttempt? {
        // Enrol when no PIN is set, else change or (null new PIN) clear it behind the current one.
        val result = withContext(Dispatchers.Default) {
            runCatching {
                if (current == null) {
                    store.enroll(checkNotNull(newPin))
                    PinAttempt.UNLOCKED
                } else {
                    store.change(current, newPin)
                }
            }.getOrNull()
        }
        if (result == PinAttempt.UNLOCKED) {
            if (newPin == null) appLock.onPinCleared() else appLock.onPinEnrolled()
            pinSet = newPin != null
            obscureSnapshot(pinSet)
            setup = false
        }
        refreshLock()
        return result
    }

    private fun refreshLock() {
        val remaining = appLock.millisUntilLock()
        locked = appLock.isLocked
        timer.removeCallbacks(deadline)
        if (!locked) remaining?.let { timer.postDelayed(deadline, it) }
    }
}

private val reasons = mapOf(
    "missing" to R.string.recovery_missing,
    "incomplete" to R.string.recovery_incomplete,
    "deleting" to R.string.recovery_deleting,
    "damaged" to R.string.recovery_damaged,
)

@Composable
private fun reasonText(reason: String) =
    stringResource(reasons[reason] ?: R.string.recovery_unavailable)

/**
 * The start screen: the active identity, the identity picker or a recovery state. `app-ready`
 * tags the status line UI tests wait for, `peer-id` the core's answer.
 */
@Composable
fun MainScreen(onLockSettings: () -> Unit = {}) {
    val context = LocalContext.current.applicationContext
    val slots = remember { RustBridge.slots(context) }
    var shown by remember { mutableStateOf(RustBridge.show(slots, slots::open)) }
    // The identity the remove screen asks about; null when it is closed.
    var removing by remember { mutableStateOf<Opened?>(null) }
    val act = { action: () -> Opened ->
        removing = null
        shown = RustBridge.show(slots, action)
    }
    removing?.let {
        RemoveScreen(slots, it, act)
        return
    }
    val opened = shown.opened
    val status = when (opened) {
        null -> R.string.status_error
        is Opened.Ready -> R.string.status_ready
        is Opened.Recovery -> R.string.status_recovery
        else -> R.string.status_pick
    }
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
        Text(stringResource(status), modifier = Modifier.testTag("app-ready"))
        when (opened) {
            null -> Text(stringResource(R.string.storage_error))
            is Opened.Ready -> {
                Text(stringResource(R.string.storage_peer_id))
                Text(opened.peerId, modifier = Modifier.testTag("peer-id"))
                // The OS gives the app no chance to warn before these erase the key.
                Text(stringResource(R.string.storage_os_loss), Modifier.testTag("os-loss"))
            }
            is Opened.Recovery -> {
                Text(reasonText(opened.reason), modifier = Modifier.testTag("recovery-reason"))
                opened.peerId?.let { Text(it, modifier = Modifier.testTag("recovery-peer")) }
                Button(onClick = { act(slots::open) }, Modifier.testTag("retry")) {
                    Text(stringResource(R.string.recovery_retry))
                }
            }
            else -> {
                for (entry in shown.entries) {
                    val (slot, label) = when (entry) {
                        is Opened.Ready -> entry.slot to entry.peerId
                        is Opened.Recovery -> entry.slot to reasonText(entry.reason)
                        else -> continue
                    }
                    val select = { slots.setActive(slot.id); slots.open() }
                    OutlinedButton(onClick = { act(select) }, Modifier.testTag("identity")) {
                        Text(label)
                    }
                }
                Button(onClick = { act(slots::create) }, Modifier.testTag("add-identity")) {
                    Text(stringResource(R.string.identity_add))
                }
            }
        }
        if (opened != null && removable(opened)) {
            OutlinedButton(onClick = { removing = opened }, Modifier.testTag("remove-identity")) {
                Text(stringResource(R.string.identity_remove))
            }
        }
        if (opened is Opened.Ready || opened is Opened.Recovery) {
            OutlinedButton(onClick = { act { Opened.Picker } }, Modifier.testTag("switch")) {
                Text(stringResource(R.string.identity_switch))
            }
        }
        OutlinedButton(onClick = onLockSettings, Modifier.testTag("lock-settings")) {
            Text(stringResource(R.string.lock_settings))
        }
    }
}

/** [Slots.remove] refuses the legacy slot and damaged or unreadable ones. */
private fun removable(opened: Opened) = when (opened) {
    is Opened.Ready -> opened.slot.id != Slots.LEGACY
    is Opened.Recovery -> opened.slot.id != Slots.LEGACY &&
        opened.reason in setOf("missing", "incomplete", "deleting")
    else -> false
}

/**
 * Second confirmation: Remove stays disabled until the checkbox is ticked. Afterwards the
 * picker shows the remaining identities; a new one is never created on its own.
 */
@Composable
private fun RemoveScreen(slots: Slots, target: Opened, act: (() -> Opened) -> Unit) {
    val (slot, peerId) = when (target) {
        is Opened.Ready -> target.slot to target.peerId
        is Opened.Recovery -> target.slot to target.peerId
        else -> return
    }
    val copies = remember {
        peerId != null && runCatching {
            slots.list().filter { it.id != slot.id }.map(slots::openSlot)
                .any { it is Opened.Ready && it.peerId == peerId }
        }.getOrDefault(false)
    }
    var confirmed by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(stringResource(R.string.remove_title), Modifier.testTag("app-ready"))
        Text(peerId ?: reasonText((target as Opened.Recovery).reason))
        if (peerId != null) {
            val copy = if (copies) R.string.remove_other_copy else R.string.remove_last_copy
            Text(stringResource(copy))
        }
        // A missing key or another copy means nothing irreplaceable is lost.
        if (peerId != null && !copies) {
            Text(stringResource(R.string.remove_warning), Modifier.testTag("remove-warning"))
        }
        Row(
            Modifier.toggleable(confirmed, role = Role.Checkbox) { confirmed = it }
                .testTag("remove-confirm"),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Checkbox(confirmed, onCheckedChange = null)
            Text(stringResource(R.string.remove_confirm))
        }
        Button(
            onClick = { act { slots.remove(slot.id, peerId); Opened.Picker } },
            Modifier.testTag("remove-action"),
            enabled = confirmed
        ) {
            Text(stringResource(R.string.remove_action))
        }
        OutlinedButton(onClick = { act(slots::open) }, Modifier.testTag("remove-cancel")) {
            Text(stringResource(R.string.remove_cancel))
        }
    }
}
