package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import java.text.DateFormat
import java.util.Date

private const val MILLIS_PER_SECOND = 1_000uL

/** Display-only conversion; timestamps and successful-check policy are owned by core. */
@Composable
internal fun updateCheckTime(seconds: ULong?): String {
    val locale = LocalConfiguration.current.locales[0]
    return when {
        seconds == null -> stringResource(R.string.update_never)
        seconds > Long.MAX_VALUE.toULong() / MILLIS_PER_SECOND -> stringResource(R.string.update_time_unavailable)
        else -> DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT, locale)
            .format(Date((seconds * MILLIS_PER_SECOND).toLong()))
    }
}
