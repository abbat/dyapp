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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
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
import androidx.compose.ui.unit.dp
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val APP_READY_TAG = "app-ready"
private const val MISSING_REASON = "missing"
private const val INCOMPLETE_REASON = "incomplete"
private const val DELETING_REASON = "deleting"

/**
 * The single activity; checks the app lock before constructing the private Compose screen and
 * offers first-launch PIN setup. PIN work runs on a background dispatcher.
 */
class MainActivity : ComponentActivity() {
    // Fail closed until onCreate reads the saved lock choice.
    private var appLock = AppLockLifecycle(true, SystemClock::elapsedRealtime)
    private val timer = Handler(Looper.getMainLooper())
    private val deadline = Runnable { refreshLock() }
    private val store by lazy { AppLockStore.onDevice(noBackupFilesDir) }
    private var locked by mutableStateOf(true)
    private var settings by mutableStateOf(false)
    private var lock by mutableStateOf(StoredLock.NEW)
    private var device by mutableStateOf(DeviceLock(false))

    // A passed OS check, not yet applied, and what it authorizes, such as a PIN reset.
    private var checked: (() -> Unit)? = null
    private var recovered by mutableStateOf(false)
    private val lockMode get() = when {
        lock == StoredLock.BIOMETRIC -> LockMode.BIOMETRIC
        recovered -> LockMode.RESET
        lock == StoredLock.PIN -> LockMode.PIN
        else -> LockMode.OFF
    }
    private val verify = { biometric: Boolean, then: () -> Unit ->
        confirmDeviceCredential(biometric) { checked = then; refreshLock() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // An unreadable verifier must fail closed, never start a new unlocked session.
        lock = runCatching { store.mode() }.getOrDefault(StoredLock.PIN)
        val lockOn = lock == StoredLock.PIN || lock == StoredLock.BIOMETRIC
        appLock = AppLockLifecycle(lockOn, SystemClock::elapsedRealtime)
        obscureSnapshot(lockOn)
        refreshLock()
        setContent {
            DYAppTheme {
                val biometric = lock == StoredLock.BIOMETRIC
                when {
                    locked && biometric -> BiometricUnlockScreen(device.secure) { verify(true) {} }
                    locked -> UnlockScreen(device.secure, ::attempt) {
                        verify(false) { recovered = true; settings = true }
                    }
                    lock == StoredLock.NEW -> SetupScreen(device, {
                        // A failed write only means the choice is offered again next launch.
                        runCatching { store.skip() }
                        lock = StoredLock.OFF
                    }) { setPin(null, it) == PinAttempt.UNLOCKED }
                    settings -> LockSettingsScreen(lockMode, device, ::setPin) {
                        settings = false
                        recovered = false
                    }
                    // Biometric settings changes need a fresh prompt, as a PIN asks for the current one.
                    biometric -> MainScreen { verify(true) { settings = true } }
                    else -> MainScreen { settings = true }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // The user may have just set a screen lock in system settings.
        val secure = getSystemService(KeyguardManager::class.java).isDeviceSecure
        device = DeviceLock(secure, biometrics()) { setPin(null, null, true) }
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
        withContext(Dispatchers.Main.immediate) {
            // A PIN checked while the app went to the background does not unlock it.
            if (result == PinAttempt.UNLOCKED && !appLock.snapshotObscured) appLock.onUnlocked()
            refreshLock()
        }
        return result
    }

    private suspend fun setPin(current: String?, newPin: String?, biometric: Boolean = false): PinAttempt? {
        val reset = recovered || lock == StoredLock.BIOMETRIC
        val result = withContext(Dispatchers.Default) {
            runCatching { store.save(current, newPin, reset, biometric) }.getOrNull()
        }
        withContext(Dispatchers.Main.immediate) {
            if (result == PinAttempt.UNLOCKED) {
                lock = if (biometric) StoredLock.BIOMETRIC else if (newPin != null) StoredLock.PIN else StoredLock.OFF
                if (lock == StoredLock.OFF) appLock.onPinCleared() else appLock.onPinEnrolled()
                obscureSnapshot(lock != StoredLock.OFF)
                recovered = false
            }
            refreshLock()
        }
        return result
    }

    private fun refreshLock() {
        // The OS check may pause the activity; apply it once the app is in the foreground again.
        checked?.takeIf { !appLock.snapshotObscured }?.let {
            checked = null
            appLock.onUnlocked()
            it()
        }
        val remaining = appLock.millisUntilLock()
        locked = appLock.isLocked
        if (locked) recovered = false
        timer.removeCallbacks(deadline)
        if (!locked) remaining?.let { timer.postDelayed(deadline, it) }
    }
}

// Switch to biometrics, or change or (null new PIN) clear the PIN behind the current one or a
// fresh OS check (reset), else enrol one.
private fun AppLockStore.save(current: String?, newPin: String?, reset: Boolean, biometric: Boolean) = when {
    biometric -> PinAttempt.UNLOCKED.also { useBiometric() }
    current != null -> change(current, newPin)
    reset -> PinAttempt.UNLOCKED.also { resetAfterDeviceCredential(newPin) }
    else -> PinAttempt.UNLOCKED.also { enroll(checkNotNull(newPin)) }
}

private val reasons = mapOf(
    MISSING_REASON to R.string.recovery_missing,
    INCOMPLETE_REASON to R.string.recovery_incomplete,
    DELETING_REASON to R.string.recovery_deleting,
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
    var editingProfile by remember { mutableStateOf(false) }
    var viewingSettings by remember { mutableStateOf(false) }
    var viewingNetwork by remember { mutableStateOf(false) }
    val act = { action: () -> Opened ->
        removing = null
        shown = RustBridge.show(slots, action)
    }
    removing?.let {
        RemoveScreen(slots, it, act)
        return
    }
    val opened = shown.opened
    val networkNeeded = rememberNetworkEntryNeeded(opened as? Opened.Ready)
    var networkDismissed by remember((opened as? Opened.Ready)?.slot?.id) { mutableStateOf(false) }
    val explicitEditor = viewingSettings || editingProfile || viewingNetwork
    val automaticNetwork = networkNeeded != false && !networkDismissed
    if (opened is Opened.Ready && (explicitEditor || automaticNetwork)) {
        when {
            viewingSettings -> IdentitySettingsScreen(slots, opened) { viewingSettings = false }
            editingProfile -> ProfileDraftScreen(opened.slot) { editingProfile = false }
            networkNeeded == null -> Text(stringResource(R.string.network_loading))
            else -> NetworkDraftScreen(opened) {
                viewingNetwork = false
                networkDismissed = true
            }
        }
        return
    }
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
        Text(stringResource(status), modifier = Modifier.testTag(APP_READY_TAG))
        IdentityDetails(shown, slots, act) { editingProfile = true }
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
        if (opened is Opened.Ready) {
            OutlinedButton(onClick = { viewingNetwork = true }, Modifier.testTag("network-draft-open")) {
                Text(stringResource(R.string.network_title))
            }
            OutlinedButton(onClick = { viewingSettings = true }, Modifier.testTag("settings-open")) {
                Text(stringResource(R.string.settings_title))
            }
        }
        OutlinedButton(onClick = onLockSettings, Modifier.testTag("lock-settings")) {
            Text(stringResource(R.string.lock_settings))
        }
    }
}

@Composable
private fun IdentityDetails(
    shown: Shown,
    slots: Slots,
    act: (() -> Opened) -> Unit,
    onProfileEdit: () -> Unit
) {
    when (val opened = shown.opened) {
        null -> { Text(stringResource(R.string.storage_error)) }
        is Opened.Ready -> { ReadyIdentityDetails(opened, onProfileEdit) }
        is Opened.Recovery -> { RecoveryIdentityDetails(opened, slots, act) }
        else -> { PickerIdentityDetails(shown.entries, slots, act) }
    }
}

@Composable
private fun ReadyIdentityDetails(opened: Opened.Ready, onProfileEdit: () -> Unit) {
    Text(stringResource(R.string.storage_peer_id))
    Text(opened.peerId, modifier = Modifier.testTag("peer-id"))
    // The OS gives the app no chance to warn before these erase the key.
    Text(stringResource(R.string.storage_os_loss), Modifier.testTag("os-loss"))
    OutlinedButton(onClick = onProfileEdit, Modifier.testTag("profile-edit")) {
        Text(stringResource(R.string.profile_title))
    }
}

@Composable
private fun RecoveryIdentityDetails(opened: Opened.Recovery, slots: Slots, act: (() -> Opened) -> Unit) {
    Text(reasonText(opened.reason), modifier = Modifier.testTag("recovery-reason"))
    opened.peerId?.let { Text(it, modifier = Modifier.testTag("recovery-peer")) }
    Button(onClick = { act(slots::open) }, Modifier.testTag("retry")) {
        Text(stringResource(R.string.recovery_retry))
    }
}

@Composable
private fun PickerIdentityDetails(entries: List<Opened>, slots: Slots, act: (() -> Opened) -> Unit) {
    for (entry in entries) {
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

// Removal refuses the legacy slot and damaged or unreadable ones.
private fun removable(opened: Opened) = when (opened) {
    is Opened.Ready -> opened.slot.id != Slots.LEGACY
    is Opened.Recovery -> opened.slot.id != Slots.LEGACY &&
        opened.reason in setOf(MISSING_REASON, INCOMPLETE_REASON, DELETING_REASON)
    else -> false
}

// Second confirmation: Remove stays disabled until checked. Afterwards the picker shows
// remaining identities; a new one is never created on its own.
@Composable
private fun RemoveScreen(slots: Slots, target: Opened, act: (() -> Opened) -> Unit) {
    val (slot, peerId) = when (target) {
        is Opened.Ready -> target.slot to target.peerId
        is Opened.Recovery -> target.slot to target.peerId
        else -> return
    }
    val copies = remember {
        peerId != null && runCatching {
            slots.list().filter { it.id != slot.id }.map { openSlot(it, slots::load) }
                .any { it is Opened.Ready && it.peerId == peerId }
        }.getOrDefault(false)
    }
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        RemoveScreenSummary(target, peerId, copies)
        RemoveScreenActions(slots, slot, peerId, act)
    }
}

@Composable
private fun RemoveScreenSummary(target: Opened, peerId: String?, copies: Boolean) {
    Text(stringResource(R.string.remove_title), Modifier.testTag(APP_READY_TAG))
    Text(peerId ?: reasonText((target as Opened.Recovery).reason))
    if (peerId != null) {
        val copy = if (copies) R.string.remove_other_copy else R.string.remove_last_copy
        Text(stringResource(copy))
    }
    // A missing key or another copy means nothing irreplaceable is lost.
    if (peerId != null && !copies) {
        Text(stringResource(R.string.remove_warning), Modifier.testTag("remove-warning"))
    }
}
