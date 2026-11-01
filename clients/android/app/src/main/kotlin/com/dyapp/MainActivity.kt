package com.dyapp

import android.app.KeyguardManager
import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal const val APP_READY_TAG = "app-ready"
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

    // The section to show when leaving the lock screen; MainScreen leaves composition behind it.
    private var returnTo by mutableStateOf<StartTarget?>(null)
    private val lockMode get() = when {
        lock == StoredLock.BIOMETRIC -> LockMode.BIOMETRIC
        recovered -> LockMode.RESET
        lock == StoredLock.PIN -> LockMode.PIN
        else -> LockMode.OFF
    }
    private val verify = { biometric: Boolean, then: () -> Unit ->
        confirmDeviceCredential(biometric) { checked = then; refreshLock() }
    }
    private val skipLock = {
        // A failed write only means the choice is offered again next launch.
        runCatching { store.skip() }
        lock = StoredLock.OFF
    }
    private val leaveSettings = {
        settings = false
        recovered = false
        returnTo = StartTarget.SETTINGS
    }
    private val attempt: suspend (String) -> PinAttempt? = { pin ->
        val result = withContext(Dispatchers.Default) { runCatching { store.attempt(pin) }.getOrNull() }
        withContext(Dispatchers.Main.immediate) {
            // A PIN checked while the app went to the background does not unlock it.
            if (result == PinAttempt.UNLOCKED && !appLock.snapshotObscured) appLock.onUnlocked()
            refreshLock()
        }
        result
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(newBase.inChosenLanguage())
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        loadTheme()
        // An unreadable verifier must fail closed, never start a new unlocked session.
        lock = runCatching { store.mode() }.getOrDefault(StoredLock.PIN)
        val lockOn = lock == StoredLock.PIN || lock == StoredLock.BIOMETRIC
        appLock = AppLockLifecycle(lockOn, SystemClock::elapsedRealtime)
        obscureSnapshot(lockOn)
        refreshLock()
        // A locked relaunch drops this at the next refresh, as any return past the lock screen.
        returnTo = takeRelaunchTarget()
        setContent {
            DYAppTheme(chosenDarkTheme()) {
                val biometric = lock == StoredLock.BIOMETRIC
                when {
                    locked && biometric -> BiometricUnlockScreen(device.secure) { verify(true) {} }
                    locked -> UnlockScreen(device.secure, attempt) {
                        verify(false) { recovered = true; settings = true }
                    }
                    lock == StoredLock.NEW -> SetupScreen(device, skipLock) { setPin(null, it) == PinAttempt.UNLOCKED }
                    settings -> LockSettingsScreen(lockMode, device, ::setPin, leaveSettings)
                    // Biometric settings changes need a fresh prompt, as a PIN asks for the current one.
                    biometric -> MainScreen(lockMode, returnTo) { verify(true) { settings = true } }
                    else -> MainScreen(lockMode, returnTo) { settings = true }
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
        if (locked) {
            recovered = false
            returnTo = null
        }
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
internal fun reasonText(reason: String) =
    stringResource(reasons[reason] ?: R.string.recovery_unavailable)

/**
 * The app shell for a ready identity, the identity step when no slot exists yet, else the
 * identity picker or a recovery state. `app-ready`
 * tags the title or status line UI tests wait for, `peer-id` the core's answer.
 */
@Composable
internal fun MainScreen(
    lockMode: LockMode = LockMode.OFF,
    start: StartTarget? = null,
    onLockSettings: () -> Unit = {}
) {
    val context = LocalContext.current.applicationContext
    val slots = remember { RustBridge.slots(context) }
    var shown by remember { mutableStateOf(RustBridge.show(slots, slots::open)) }
    // The identity the remove dialog asks about; null when it is closed.
    var removing by remember { mutableStateOf<Opened?>(null) }
    var editor by remember { mutableStateOf(start.takeIf { shown.opened is Opened.Ready }) }
    val act: (() -> Opened) -> Unit = { action ->
        removing = null
        shown = RustBridge.show(slots, action)
        // Settings stays open across Create and Switch, but not past a removal.
        if (shown.opened !is Opened.Ready) editor = null
    }
    val opened = shown.opened
    val open: (StartTarget?) -> Unit = { target ->
        when (target) {
            StartTarget.REMOVE -> removing = opened
            StartTarget.LOCK -> onLockSettings()
            else -> editor = target
        }
    }
    when {
        opened is Opened.Ready -> AppShell(slots, opened, editor, lockMode, act, open)
        opened == Opened.Setup -> WizardIdentity { act(slots::create) }
        else -> StartColumn(shown, slots, act, open)
    }
    removing?.let { RemoveDialog(slots, it, act) { removing = null } }
}

// Removal refuses the legacy slot and damaged or unreadable ones.
internal fun removable(opened: Opened) = when (opened) {
    is Opened.Ready -> opened.slot.id != Slots.LEGACY
    is Opened.Recovery -> opened.slot.id != Slots.LEGACY &&
        opened.reason in setOf(MISSING_REASON, INCOMPLETE_REASON, DELETING_REASON)
    else -> false
}
