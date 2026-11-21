package com.dyapp

import androidx.compose.runtime.Composable

@Composable
internal fun ReadReceiptsRow(active: Opened.Ready) = PrivacyPreferenceRow(active, PrivacyPreferenceKind.RECEIPTS)
