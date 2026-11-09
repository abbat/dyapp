# UI state retention contract

This page records what the apps preserve today. It does not promise recovery for state that has
not been saved. The shared [client Store](../design/client-local-storage.md) persists accepted
writes per identity; each platform decides when to submit an editor or composer draft.

## Desktop module boundaries

`locale.js` owns catalogue loading and exports live `locale`, `pseudoLocale` and `strings`
bindings. The profile form, interests and seeking modules import their catalogue and helpers
explicitly. `profile-fields.js` owns the current scale metadata and shared range validation;
`peer-labels.js` privately owns the peers used to disambiguate short IDs. Chat and search add
peers through `addKnownPeers`; reloading the identities replaces that set through `knowPeers`.
`sticker-pixels.js` exports the validated canvas builders. These modules publish no page globals.
The page loads one native ES-module entry, `entry.js`. Controllers import their helpers, live
bindings and controller APIs explicitly; page globals and classic-script order are not used.
`identity-state.js` owns the selected identity and generation, `chat-state.js` owns shared chat
state, and independent clipboard, place and DOM modules avoid eager controller cycles.
Their JSDoc DTOs distinguish ready/recovery identity state, chat history/messages and text-forward
versus profile-share targets. The native bridge associates described command names with their
argument tuples and result DTOs; undescribed command results remain `unknown`.
`dom.js` checks the kind of elements returned by typed lookups. Required ID/selector lookups
fail on a missing or wrong-kind element; optional selector lookups retain `null`, and empty
selector lists remain valid. The helpers preserve static `NodeList` results and use JSDoc types
without widening element contracts to `any`.
Locale loading validates the catalogue object and string values before replacing the live locale
state. Invalid catalogues leave that state intact. Sticker rendering rejects an unavailable
canvas context explicitly.
Startup still waits for `stringsShown`. Rust mail/revocation notifications dispatch the DOM events
`dyapp:mail-updated` and `dyapp:device-revoked`; the entry module routes them to their controllers.
Native smoke has its own local interface and async scope, included only with `ui-smoke`.
Locale selection, validation and native CSP are unchanged. Modules are served directly, without
a bundler.

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
recover unsaved edits after a window reload, crash or process termination. The tests exercise
unsaved field and invalid-place retention across section navigation, a normal restart after Save,
and cancellation of the publish dialog; they do not inject process death.
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
