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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics

/** Completed screen revision; zero while registry work is pending. */
internal val identityReadyRevisionKey = SemanticsPropertyKey<Long>("IdentityReadyRevision")

private class IdentityNavigation(
    start: StartTarget?,
    private val actions: IdentityActions,
    private val onLock: () -> Unit
) {
    var removing by mutableStateOf<Opened?>(null)
        private set
    var editor by mutableStateOf(start)
        private set

    fun completed(shown: Shown) {
        if (shown.opened !is Opened.Ready) editor = null
    }

    fun act(action: () -> Opened) {
        dismiss()
        actions.perform(action, ::completed)
    }

    fun open(opened: Opened?, target: StartTarget?) {
        when (target) {
            StartTarget.REMOVE -> removing = opened
            StartTarget.LOCK -> onLock()
            else -> editor = target
        }
    }

    fun dismiss() { removing = null }
}

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
    val navigation = remember(actions) { IdentityNavigation(start, actions) { onLock() } }
    val state by actions.state.collectAsState()
    LaunchedEffect(actions) { actions.initialize(slots::open, navigation::completed) }
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
private fun IdentityContent(shown: Shown, slots: Slots, navigation: IdentityNavigation, lockMode: LockMode) {
    val opened = shown.opened
    val open: (StartTarget?) -> Unit = { navigation.open(opened, it) }
    when {
        opened is Opened.Ready -> AppShell(slots, opened, navigation.editor, lockMode, navigation::act, open)
        opened == Opened.Setup -> WizardIdentity { navigation.act(slots::create) }
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
