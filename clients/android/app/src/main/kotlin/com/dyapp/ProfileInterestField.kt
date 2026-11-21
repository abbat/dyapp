package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.profileInterestLimits

/** Kotlin retains typed input; canonicalization and admission belong to the common core. */
@Composable
internal fun ProfileInterestField(typed: MutableState<String>) {
    val limits = remember { profileInterestLimits() }
    FormCard {
        OutlinedTextField(
            typed.value,
            { typed.value = it },
            Modifier.fillMaxWidth().testTag("profile-interests"),
            label = { Text(stringResource(R.string.profile_interests)) },
            supportingText = {
                Text(
                    stringResource(
                        R.string.profile_interests_hint,
                        limits.maxTags.toString(),
                        limits.maxTagChars.toString()
                    )
                )
            }
        )
    }
}
