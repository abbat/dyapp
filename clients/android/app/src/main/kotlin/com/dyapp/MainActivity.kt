package com.dyapp

import android.app.KeyguardManager
import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.resumeNetworkSelection

internal const val APP_READY_TAG = "app-ready"

/**
 * The single activity; checks the app lock before constructing the private Compose screen and
 * offers first-launch PIN setup. PIN work runs on a background dispatcher.
 */
class MainActivity : ComponentActivity() {
    // Fail closed until asynchronous startup reads the saved lock choice.
    private var appLock = AppLockLifecycle(true, SystemClock::elapsedRealtime)
    private var lockReady = false
    private val networkAccess = LocalNetworkAccess(this)
    private val content = PrivateContent()
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
        confirmDeviceCredential(biometric) {
            checked = then
            refreshLock()
        }
    }
    private val skipLock: suspend () -> Boolean = {
        val mode = withContext(Dispatchers.IO) {
            runCatching { store.skip() }.getOrElse { runCatching { store.mode() }.getOrDefault(StoredLock.PIN) }
        }
        content.skipped(mode)
        mode == StoredLock.OFF
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
        window.callback = WindowKeyGate(requireNotNull(window.callback), ::consumeKey)
        loadTheme()
        // Protect the window before asynchronous storage admission, including on a cold launch.
        obscureSnapshot(true)
        refreshLock()
        // A locked relaunch drops this at the next refresh, as any return past the lock screen.
        returnTo = takeRelaunchTarget()
        setContent { content.Show() }
    }

    private inner class PrivateContent {
        fun skipped(mode: StoredLock) {
            if (mode == StoredLock.PIN || mode == StoredLock.BIOMETRIC) {
                appLock.close()
                appLock = AppLockLifecycle(true, SystemClock::elapsedRealtime)
                if (!lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                    appLock.onBackground()
                }
            }
            initializeLock(mode)
        }

        fun initializeLock(mode: StoredLock) {
            lock = mode
            if (mode == StoredLock.NEW || mode == StoredLock.OFF) appLock.onPinCleared()
            obscureSnapshot(mode == StoredLock.PIN || mode == StoredLock.BIOMETRIC)
            lockReady = true
            refreshLock()
        }

        @Composable
        fun Show() {
            LaunchedEffect(locked, lock) {
                if (!locked && lock != StoredLock.NEW) networkAccess.firstUnlock()
            }
            val settingsNavigation = rememberSettingsNavigation()
            ProvideLocalNetworkAccess(networkAccess) {
                CompositionLocalProvider(LocalSettingsNavigation provides settingsNavigation) {
                    PrivateScreens()
                }
            }
        }

        @Composable
        private fun PrivateScreens() {
            DYAppTheme(chosenDarkTheme()) {
                AppLockStartup(this@MainActivity, { store.mode() }, ::initializeLock) {
                    ReadyScreens()
                }
            }
        }

        @Composable
        private fun ReadyScreens() {
            when {
                locked -> Locked()
                lock == StoredLock.NEW -> SetupScreen(
                    configurationDevice(),
                    skipLock
                ) { setPin(null, it) == PinAttempt.UNLOCKED }
                settings -> Settings()
                else -> Main()
            }
        }

        @Composable
        private fun Settings() {
            val mode = lockMode
            val reset = mode == LockMode.RESET || mode == LockMode.BIOMETRIC
            LockSettingsScreen(mode, configurationDevice(reset), { current, newPin ->
                setPin(current, newPin, reset = reset)
            }, leaveSettings)
        }

        private fun configurationDevice(reset: Boolean = false) = DeviceLock(device.secure, device.biometrics) {
            setPin(null, null, biometric = true, reset = reset) == PinAttempt.UNLOCKED
        }

        @Composable
        private fun Locked() {
            if (lock == StoredLock.BIOMETRIC) {
                BiometricUnlockScreen(device.secure) { verify(true) {} }
            } else {
                UnlockScreen(device.secure, attempt) {
                    verify(false) {
                        recovered = true
                        settings = true
                    }
                }
            }
        }

        @Composable
        private fun Main() {
            // Biometric settings changes need a fresh prompt, as a PIN asks for the current one.
            if (lock == StoredLock.BIOMETRIC) {
                MainScreen(lockMode, returnTo) { verify(true) { settings = true } }
            } else {
                MainScreen(lockMode, returnTo) { settings = true }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // The user may have just set a screen lock in system settings.
        val secure = getSystemService(KeyguardManager::class.java).isDeviceSecure
        device = DeviceLock(secure, biometrics())
        appLock.onForeground()
        lifecycleScope.launch(Dispatchers.IO) { runCatching { resumeNetworkSelection() } }
        refreshLock()
    }

    override fun onPause() {
        appLock.onBackground()
        refreshLock()
        super.onPause()
    }

    override fun onDestroy() {
        networkAccess.close()
        timer.removeCallbacks(deadline)
        appLock.close()
        super.onDestroy()
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        val wasLocked = locked
        appLock.onInteraction()
        refreshLock()
        return if (!wasLocked && locked) true else super.dispatchTouchEvent(event)
    }

    private fun consumeKey(): Boolean {
        val wasLocked = locked
        appLock.onInteraction()
        refreshLock()
        return !wasLocked && locked
    }

    private suspend fun setPin(
        current: String?,
        newPin: String?,
        biometric: Boolean = false,
        reset: Boolean = false,
    ): PinAttempt? {
        val (result, mode) = withContext(Dispatchers.Default) {
            val result = runCatching { store.save(current, newPin, reset, biometric) }.getOrNull()
            val mode = runCatching { store.mode() }
            result.takeIf { mode.isSuccess } to mode.getOrDefault(StoredLock.PIN)
        }
        withContext(Dispatchers.Main.immediate) {
            if (result == PinAttempt.UNLOCKED) {
                lock = mode
                if (lock == StoredLock.OFF) appLock.onPinCleared() else appLock.onPinEnrolled()
                obscureSnapshot(lock != StoredLock.OFF)
                recovered = false
            } else if (result == null) {
                content.skipped(mode)
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
            if (lockReady) returnTo = null
        }
        timer.removeCallbacks(deadline)
        if (!locked) remaining?.let { timer.postDelayed(deadline, it) }
    }
}

// Switch to biometrics, or change or (null new PIN) clear the PIN behind the current one or a
// fresh OS check (reset), else request initial-only enrollment under core admission.
private fun AppLockStore.save(current: String?, newPin: String?, reset: Boolean, biometric: Boolean) = when {
    biometric -> PinAttempt.UNLOCKED.also { useBiometric(afterDeviceCredential = reset) }
    current != null -> change(current, newPin)
    reset -> PinAttempt.UNLOCKED.also { resetAfterDeviceCredential(newPin) }
    else -> PinAttempt.UNLOCKED.also { enroll(checkNotNull(newPin)) }
}

private val reasons = mapOf(
    MISSING_REASON to R.string.recovery_missing,
    INCOMPLETE_REASON to R.string.recovery_incomplete,
    DELETING_REASON to R.string.recovery_deleting,
    "damaged" to R.string.recovery_damaged,
    "invalidPointer" to R.string.recovery_selection,
)

@Composable
internal fun reasonText(reason: String) =
    stringResource(reasons[reason] ?: R.string.recovery_unavailable)
