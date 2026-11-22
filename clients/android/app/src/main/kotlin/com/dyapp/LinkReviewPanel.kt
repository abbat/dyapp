package com.dyapp

import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.launch
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.profileScales

private class LinkReviewControls(val canLeave: Boolean, val back: () -> Unit, val save: () -> Unit)

/** Complete owner choices remain separate from the receiving receipt and enrollment completion. */
@Composable
internal fun LinkReviewPanel(snapshot: LinkReceiverSnapshot, actions: LinkEnrollmentActions) {
    val opening = snapshot.enrollment.comparison ?: return
    if (!snapshot.enrollment.offered || snapshot.enrollment.completed) return
    key(opening) {
        var visible by remember { mutableStateOf(true) }
        if (snapshot.enrollment.saved != null) {
            Text(stringResource(R.string.link_review_saved), Modifier.testTag("link-review-saved"))
        } else if (visible) {
            LinkReviewForm(opening, snapshot, actions) { visible = false }
        } else {
            val show = { visible = true }
            Button(show, Modifier.testTag("link-review-open").applicationClick(show), !snapshot.busy) {
                Text(stringResource(R.string.link_review_show_comparison))
            }
        }
    }
}

@Composable
private fun LinkReviewForm(
    opening: LinkReviewOpening,
    snapshot: LinkReceiverSnapshot,
    actions: LinkEnrollmentActions,
    back: () -> Unit
) {
    val form = rememberReviewForm(opening, snapshot)
    val photos = remember(form) { LinkReviewPhotoState(form) }
    LaunchedEffect(form) {
        if (LinkReviewField.PHOTOS in opening.snapshot.differences) photos.loadOffered()
    }
    LinkReviewContent(form, photos, rememberReviewControls(snapshot, form, actions, back))
}

@Composable
private fun rememberReviewForm(opening: LinkReviewOpening, snapshot: LinkReceiverSnapshot): LinkReviewFormState {
    val current by rememberUpdatedState(snapshot)
    val form = remember(opening) {
        LinkReviewFormState(opening) {
            !current.busy && current.enrollment.canConfirm && current.enrollment.comparison === opening
        }
    }
    DisposableEffect(form) { onDispose(form::retire) }
    return form
}

@Composable
private fun rememberReviewControls(
    snapshot: LinkReceiverSnapshot,
    form: LinkReviewFormState,
    actions: LinkEnrollmentActions,
    back: () -> Unit
): LinkReviewControls {
    val current by rememberUpdatedState(snapshot)
    val scope = rememberCoroutineScope()
    return LinkReviewControls(
        canLeave = !snapshot.busy && !form.submitting,
        back = { if (form.present && !current.busy && !form.submitting) back() },
        save = {
            form.beginConfirmation(actions)?.let { result ->
                scope.launch {
                    form.confirmationCompleted(result.await())
                }
            }
        }
    )
}

@Composable
private fun LinkReviewContent(form: LinkReviewFormState, photos: LinkReviewPhotoState, controls: LinkReviewControls) {
    val scales = remember { profileScales() }
    CardTitle(R.string.link_review_title)
    Text(stringResource(R.string.link_review_intro))
    Caption(stringResource(R.string.link_review_authority))
    Caption(stringResource(R.string.link_review_no_publish))
    form.failure?.let { failure ->
        ErrorNotice(
            stringResource(R.string.link_review_preview_error),
            "link-review-preview-error",
            failure.toString(),
            "link-review-preview-details",
            "link-review-preview-copy"
        )
    }
    if (form.opening.snapshot.differences.isEmpty()) Text(stringResource(R.string.link_review_equal))
    val preview = form.preview
    if (preview != null) CardTitle(R.string.link_review_preview_title)
    for (field in form.opening.snapshot.differences) {
        if (preview == null) {
            LinkReviewChoiceCard(form, field, photos, scales)
        } else {
            LinkReviewPreviewCard(form, field, photos, scales)
        }
    }
    LinkReviewFooter(form, controls)
}

@Composable
private fun LinkReviewPreviewControl(form: LinkReviewFormState, focus: LinkReviewFocus) {
    if (form.preview == null) {
        val preview = form::showPreview
        Button(
            preview,
            Modifier.testTag("link-review-preview").focusRequester(focus.preview)
                .focusProperties { canFocus = true }
                .applicationClick(preview),
            form.available && form.complete
        ) {
            Text(stringResource(R.string.link_review_preview))
        }
    } else {
        val edit = form::editPreview
        Button(
            edit,
            Modifier.testTag("link-review-edit").focusRequester(focus.edit)
                .focusProperties { canFocus = true }
                .applicationClick(edit),
            form.available
        ) {
            Text(stringResource(R.string.link_review_edit))
        }
    }
}

@Composable
private fun LinkReviewFooter(form: LinkReviewFormState, controls: LinkReviewControls) {
    LinkReviewPreviewControl(form, rememberLinkReviewFocus(form.preview != null, form.available))
    Button(
        controls.save,
        Modifier.testTag("link-review-confirm").applicationClick(controls.save),
        form.available && form.complete
    ) { Text(stringResource(R.string.link_review_save)) }
    TextButton(controls.back, Modifier.testTag("link-review-back").applicationClick(controls.back), controls.canLeave) {
        Text(stringResource(R.string.link_review_back_receipt))
    }
}
