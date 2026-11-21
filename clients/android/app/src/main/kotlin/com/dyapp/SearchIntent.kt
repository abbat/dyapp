package com.dyapp

import uniffi.dyapp.SearchCondition
import uniffi.dyapp.SearchEmployer
import uniffi.dyapp.searchMutualConditions

/** Immutable native query values captured together on UI admission, before backend readiness. */
internal data class SearchIntent(
    val choices: List<SearchCondition>,
    val employer: SearchEmployer,
    val mutual: Boolean,
)

/** The saved own profile is read in IO; native core owns mutual eligibility and conditions. */
internal fun SearchIntent.conditions(opened: Opened.Ready): List<SearchCondition> {
    val mutualChoices = if (mutual) {
        searchMutualConditions(loadDraft(opened).record())
            .ifEmpty { throw IllegalArgumentException("mutual unavailable") }
    } else {
        emptyList()
    }
    return choices + mutualChoices
}

/** Even an invalid field is frozen as the admitted failure, not reread after the user edits it. */
internal fun SearchFilters.captureIntent(): Result<SearchIntent> = runCatching {
    SearchIntent(conditions(), employer.employer(), mutual && mutualOffered)
}
