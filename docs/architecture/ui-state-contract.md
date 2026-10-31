# UI state retention contract

This page records what the apps preserve today. It does not promise recovery for state that has
not been saved. The shared [client Store](../design/client-local-storage.md) persists accepted
writes per identity; each platform decides when to submit an editor or composer draft.

## Linux desktop profile editor

| State | Switching app sections in the same window | Closing and reopening the app |
|---|---|---|
| Fields after **Save draft** or **Preview and publish** succeeds | Kept | Loaded from the active identity's Store |
| Edits since the last successful save | Kept in the existing WebView form | Not restored; the last saved draft loads |
| Publish-dialog consent checkboxes | Cleared on each open | Cleared |

A saved profile draft is separate from a published profile. Save validates the form, including
any entered place against its suggestions; invalid input stays in the form and is not written
to the Store.
Switching identities reloads that identity's saved draft. The current UI makes no promise to
recover unsaved edits after a window reload, crash or process termination. The tests exercise a
normal restart after Save and cancellation of the publish dialog; they do not inject process death.
See the [Linux UI smoke](../testing/ui-testing.md#linux).

## Chat composer

The Linux Chats composer is planned. No message-draft survival guarantee is implemented yet.
Its contract must be set when the Chats UI and per-identity draft storage are added; a receipt or
outbox write is not a substitute for a visible unsent draft.

## Open design question

Saving only after an explicit action avoids persisting invalid intermediate input, but a crash can
lose edits. Automatic recovery would need a per-identity representation of partial form values and
an explicit rule for when to clear them. Mobile lifecycle behavior needs a separate audit before a
cross-platform guarantee is stated.
