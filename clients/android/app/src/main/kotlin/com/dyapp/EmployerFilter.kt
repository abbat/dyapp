package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.SearchEmployerMode
import uniffi.dyapp.searchEmployerFilter

/** The employer choices with their test tags and labels. */
internal enum class EmployerKind(val tag: String, val label: Int, val mode: SearchEmployerMode) {
    ANY("search-employer-any", R.string.search_employer_any, SearchEmployerMode.ANY),
    SAME("search-employer-same", R.string.search_employer_same, SearchEmployerMode.SAME),
    EXCLUDE("search-employer-exclude", R.string.search_employer_exclude, SearchEmployerMode.EXCLUDE),
}

/** The employer filter: it narrows verified results on this device, so it is shown whatever the nodes offer. */
internal class EmployerFilter {
    var kind by mutableStateOf(EmployerKind.ANY)
    var company by mutableStateOf("")

    /** The shared core interprets raw text; the platform owns only fields and labels. */
    fun employer() = searchEmployerFilter(kind.mode, company)
}

@Composable
internal fun EmployerFilterFields(filter: EmployerFilter) {
    Column(Modifier.selectableGroup()) {
        Text(stringResource(R.string.search_employer))
        EmployerKind.entries.forEach { ChoiceRow(it.tag, it.label, filter.kind == it) { filter.kind = it } }
    }
    OutlinedTextField(
        filter.company,
        { filter.company = it },
        Modifier.fillMaxWidth().testTag("search-company"),
        label = { Text(stringResource(R.string.search_company)) },
        singleLine = true
    )
    Caption(stringResource(R.string.search_employer_notice), Modifier.testTag("search-employer-notice"))
}
