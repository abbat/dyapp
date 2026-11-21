package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ProfileScale

@Composable
internal fun reviewText(value: String) = value.ifEmpty { stringResource(R.string.link_review_unpublished) }

@Composable
internal fun reviewNumber(value: UInt?) = value?.toString() ?: stringResource(R.string.link_review_unpublished)

@Composable
internal fun reviewBoolean(value: Boolean?) = stringResource(
    when (value) {
        null -> R.string.link_review_unpublished
        true -> R.string.link_review_yes
        false -> R.string.link_review_no
    }
)

@Composable
internal fun reviewRow(label: Int, value: String) =
    stringResource(R.string.profile_preview_row, stringResource(label), value)

@Composable
internal fun reviewScale(key: String, value: UInt, scales: List<ProfileScale>): String {
    if (value == 0u) return stringResource(R.string.link_review_unpublished)
    val choice = scales.firstOrNull { it.key == key }?.choices?.firstOrNull { it.code == value }
    return choice?.let { label("value.$key.${it.key}") }
        ?: stringResource(R.string.link_review_unknown_value, value.toString())
}

@Composable
internal fun reviewNamedCode(name: String?, code: String): String =
    if (name.isNullOrEmpty() || name == code) {
        reviewText(code)
    } else {
        stringResource(R.string.link_review_named_code, name, code)
    }
