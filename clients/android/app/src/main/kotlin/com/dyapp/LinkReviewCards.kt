package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.ProfileScale

@Composable
internal fun linkReviewSourceLabel(source: LinkReviewSource) = stringResource(
    when (source) {
        LinkReviewSource.LOCAL -> R.string.link_review_local
        LinkReviewSource.REMOTE -> R.string.link_review_remote
    }
)

@Composable
internal fun LinkReviewChoiceCard(
    form: LinkReviewFormState,
    field: LinkReviewField,
    photos: LinkReviewPhotoState,
    scales: List<ProfileScale>
) {
    FormCard(Modifier.testTag("link-review-field-${field.name}").selectableGroup()) {
        CardTitle(linkReviewLabels.getValue(field))
        for ((source, profile) in listOf(
            LinkReviewSource.LOCAL to form.opening.snapshot.local,
            LinkReviewSource.REMOTE to form.opening.snapshot.remote
        )) {
            LinkReviewRadio(form, field, source, linkReviewValue(field, profile, scales))
            LinkReviewFullValue(field, profile, scales, source)
            if (field == LinkReviewField.PHOTOS) LinkReviewPhotoList(form, photos, source, profile)
        }
    }
}

@Composable
private fun LinkReviewRadio(
    form: LinkReviewFormState,
    field: LinkReviewField,
    source: LinkReviewSource,
    value: String
) {
    val sourceLabel = linkReviewSourceLabel(source)
    val description = stringResource(
        R.string.profile_preview_row, stringResource(linkReviewLabels.getValue(field)), sourceLabel
    ) + "\n" + value
    val choose = { form.choose(field, source) }
    val selected = form.source(field) == source
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp)
            .selectable(selected, enabled = form.available, role = Role.RadioButton, onClick = choose)
            .testTag("link-review-${field.name}-${source.name}")
            .semantics { contentDescription = description }
            .applicationClick(choose),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        RadioButton(selected, onClick = null, enabled = form.available)
        Text(sourceLabel, Modifier.clearAndSetSemantics {})
    }
}

@Composable
internal fun LinkReviewPreviewCard(
    form: LinkReviewFormState,
    field: LinkReviewField,
    photos: LinkReviewPhotoState,
    scales: List<ProfileScale>
) {
    val profile = form.preview ?: return
    val source = form.source(field) ?: return
    FormCard(Modifier.testTag("link-review-chosen-${field.name}")) {
        CardTitle(linkReviewLabels.getValue(field))
        Caption(linkReviewSourceLabel(source))
        LinkReviewFullValue(field, profile, scales, source)
        if (field == LinkReviewField.PHOTOS) LinkReviewPhotoList(form, photos, source, profile)
    }
}

@Composable
private fun LinkReviewFullValue(
    field: LinkReviewField,
    profile: LinkReviewProfile,
    scales: List<ProfileScale>,
    source: LinkReviewSource
) = Text(
    linkReviewValue(field, profile, scales),
    Modifier.fillMaxWidth().testTag("link-review-${field.name}-${source.name}-value")
)
