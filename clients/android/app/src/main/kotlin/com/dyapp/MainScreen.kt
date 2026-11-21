package com.dyapp

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics

/** Completed screen revision; zero while registry work is pending. */
internal val identityReadyRevisionKey = SemanticsPropertyKey<Long>("IdentityReadyRevision")

/** Existing identity routes; registry work runs off the UI thread, including lock admission. */
@Composable
internal fun MainScreen(
    lockMode: LockMode = LockMode.OFF,
    start: StartTarget? = null,
    onLockSettings: () -> Unit = {}
) {
    val context = LocalContext.current.applicationContext
    val slots = remember { RustBridge.slots(context) }
    val scope = rememberCoroutineScope()
    val actions = remember(slots, scope) { IdentityActions(scope) { RustBridge.show(slots, it) } }
    val onLock by rememberUpdatedState(onLockSettings)
    val receiving = rememberSaveable(stateSaver = receiveWizardIntentSaver) { mutableStateOf(ReceiveWizardIntent()) }
    val navigation = remember(actions, receiving) { IdentityNavigation(start, actions, receiving) { onLock() } }
    val state by actions.state.collectAsState()
    ObserveIdentityCleanup(actions, slots, navigation)
    Box(
        Modifier.testTag("identity-state").semantics {
            set(identityReadyRevisionKey, if (state.loading) 0L else state.revision)
        }
    ) {
        if (state.loading) {
            IdentityLoading()
        } else {
            IdentityContent(
                checkNotNull(state.shown),
                slots,
                navigation,
                lockMode
            )
        }
    }
}

@Composable
private fun ObserveIdentityCleanup(actions: IdentityActions, slots: Slots, navigation: IdentityNavigation) {
    LaunchedEffect(actions) {
        actions.initialize(slots::open, navigation::completed).join()
        DeviceCleanupEvents.events.collect { events ->
            if (events.isNotEmpty()) {
                actions.devicesCleaned(events, slots::open, navigation::completed)
                DeviceCleanupEvents.observed(events)
            }
        }
    }
}

@Composable
private fun IdentityContent(shown: Shown, slots: Slots, navigation: IdentityNavigation, lockMode: LockMode) {
    IdentityDurabilityNotice(shown)
    val opened = shown.opened
    val open: (StartTarget?) -> Unit = { navigation.open(opened, it) }
    val receiving = navigation.receiveIntent
    when {
        opened is Opened.Ready && receiving.matches(opened) -> FirstRunReceiveScreen(
            opened,
            LinkReceiveNavigation(
                { navigation.leaveReceiving(receiving) },
                { navigation.leaveReceiving(receiving, slots::open) },
                { navigation.leaveReceiving(receiving) { Opened.Picker } }
            )
        )
        opened is Opened.Ready -> AppShell(
            slots,
            opened,
            navigation.editor,
            ShellControls(lockMode, navigation::act, open)
        )
        opened == Opened.Setup -> WizardIdentity(
            onReceive = { navigation.createFromSetup(shown, slots::create, receive = true) },
            onCreate = { navigation.createFromSetup(shown, slots::create, receive = false) }
        )
        else -> StartColumn(shown, slots, navigation::act, open)
    }
    navigation.removing?.let { RemoveDialog(slots, it, navigation::act, navigation::dismiss) }
}

@Composable
private fun IdentityLoading() {
    Page(actions = {}) {
        PageTitle(R.string.app_name)
        CircularProgressIndicator(Modifier.testTag("identity-loading"))
        Text(stringResource(R.string.view_photo_loading))
    }
}
