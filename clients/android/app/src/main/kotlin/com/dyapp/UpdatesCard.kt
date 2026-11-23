package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdatePlatform
import uniffi.dyapp.MobileUpdateState

private val intervals = listOf(
    Triple("update-6h", R.string.update_6h, MobileUpdateInterval.SIX_HOURS),
    Triple("update-12h", R.string.update_12h, MobileUpdateInterval.TWELVE_HOURS),
    Triple("update-24h", R.string.update_24h, MobileUpdateInterval.DAY),
    Triple("update-7d", R.string.update_7d, MobileUpdateInterval.WEEK),
)

/** Installation metadata is independent of identities; no request is admitted here. */
@Composable
internal fun UpdatesCard() {
    val root = LocalContext.current.applicationContext.noBackupFilesDir.absolutePath
    UpdateMetadataCard(root) {
        MobileClientUpdates(root, MobileUpdatePlatform.ANDROID).use { it.readState() }
    }
}

/** Owner-keyed reads discard completion after the card leaves or changes ownership. */
@Composable
internal fun UpdateMetadataCard(
    owner: Any,
    readPermission: (Context) -> Boolean = ::updateNoticePermission,
    read: () -> MobileUpdateState,
) {
    val loaded by rememberNativeRead(owner, read = read)
    val snapshot = loaded?.getOrNull()
    FormCard(Modifier.testTag("updates")) {
        CardTitle(R.string.settings_updates)
        Column(Modifier.selectableGroup()) {
            Text(stringResource(R.string.update_interval))
            intervals.forEach { (tag, label, interval) ->
                ChoiceRow(tag, label, snapshot?.interval == interval, enabled = false) {}
            }
        }
        UpdateReadStatus(loaded)
        UpdateAvailabilityLabel(snapshot)
        UpdateNoticePermission(owner, snapshot, readPermission)
        OutlinedButton(onClick = {}, Modifier.testTag("update-check"), enabled = false) {
            Text(stringResource(R.string.update_check))
        }
    }
}

@Composable
private fun UpdateReadStatus(loaded: Result<MobileUpdateState>?) {
    val text = when {
        loaded == null -> stringResource(R.string.update_metadata_loading)
        loaded.isFailure -> stringResource(R.string.update_metadata_failed)
        else -> stringResource(R.string.update_last_check, updateCheckTime(loaded.getOrThrow().lastSuccess))
    }
    Caption(text, Modifier.testTag("update-metadata"))
}
