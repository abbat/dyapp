# Desktop UX references

> **Status:** research, not a decision. Checked 2026-10-28. This page collects what platform
> guidelines and comparable apps do, as input for the v0.0.1 desktop redesign. Decisions follow
> later in the information architecture (IA) and the wireframes; section 8 only lists candidates.

## Purpose and rules

- **Purpose:** give the redesign of `clients/desktop/ui/` (Tauri webview, Linux and Windows) a
  common evidence base: platform rules, key-as-account apps, network entry, profile and search,
  settings and destructive actions.
- **Patterns only.** No code, strings, icons or artwork are copied. Most reference apps are
  GPL/AGPL/MPL; dyapp is MIT. Strings below are paraphrased. Screenshots are linked, never stored.
- **Dating:** all findings were checked on 2026-10-28. Pages can change or rot; re-check before
  quoting.
- **Evidence grades:** "read" means the page or source file was read; "snippet" means a search
  result summary only; "caption only" means a screenshot judged from its caption or alt text.
  Section 10 lists what failed.
- **Context:** current screens are described in [v0.0.1-client-screens.md](v0.0.1-client-screens.md);
  identity storage rules in
  [desktop-identity-storage.md](../development/desktop-identity-storage.md).

## 1. How to read this page

| Section | Content |
|---------|---------|
| 2 | Platform guidelines per topic and the common denominator |
| 3 | Key-as-account apps: flows, patterns, lessons |
| 4 | Network entry and connection status |
| 5 | Profile editing, publish confirmation, search |
| 6 | Settings structure and destructive actions |
| 7 | Screenshot index per flow (links only) and gaps |
| 8 | Candidate decisions: table, then per decision recommendation and trade-offs |
| 9 | Open questions for the maintainer |
| 10 | Limits of the research |

## 2. Platform guidelines

Platforms: GNOME HIG with libadwaita, Windows (WinUI, Fluent), Apple HIG (macOS), Material 3
(Android parity), KDE HIG, plus the web platform (WCAG, MDN). A webview cannot render native
widgets, so "feels native" means matching behaviour (keyboard, focus, Esc and Enter, shortcuts,
system theme and scale), not pixels.

### 2.1 Navigation

| Platform | Rule |
|----------|------|
| GNOME | Sidebar when views exceed a view switcher, locations change, or users move back and forth; avoid for immersive content; order by usefulness. Split view collapses so the sidebar becomes the root page. Smallest desktop size to support is 1024x600; cap content width. |
| Windows | NavigationView: minimal (menu button only) at 640 px or less, compact icon rail 641-1007 px, expanded pane above. Settings is the last item, pinned to the bottom. Content margin 12 px minimal, 24 px otherwise. |
| Apple | Sidebar for top-level areas; let people hide it; group deep hierarchy with disclosure; compact control when space is tight. |
| Material 3 | Compact (under 600 dp) bottom bar; medium and expanded rail (600-1199); drawer optional. Rail destination counts unverified. |
| KDE | Landing page only: "feel at home" and "work well outside Plasma". |

- **Common denominator:** left sidebar (icon and label) for 3+ areas, Settings last, collapse to a
  rail or off-canvas menu near 640 px. dyapp has 4-6 destinations (identity, network, node, profile,
  search, settings), which fits.
- Sources: https://developer.gnome.org/hig/patterns/nav/sidebars.html,
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/adaptive-layouts.html,
  https://developer.gnome.org/hig/guidelines/adaptive.html,
  https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview,
  https://developer.apple.com/design/human-interface-guidelines/sidebars,
  https://developer.android.com/develop/ui/compose/layouts/adaptive/use-window-size-classes,
  https://developer.android.com/develop/ui/compose/layouts/adaptive/build-adaptive-navigation,
  https://develop.kde.org/hig/

### 2.2 Layout and spacing

| Platform | Rule |
|----------|------|
| Windows | Scrollable single-column pages; content width about 1000-1100 px; 4 px base spacing (from general Fluent knowledge, the spacing page 404'd); group settings under section headers; 50-60 characters per line. |
| GNOME | Constrain content with framing containers; lists get min and max widths; boxed lists group related rows. |

- **Common denominator:** left-aligned single column, 4 px spacing scale, text width about 40rem,
  page about 60rem, `rem` and `ch` units. Today's `main { max-width: 960px; text-align: center }`
  does not match; never centre forms.
- Sources: https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings,
  https://developer.gnome.org/hig/patterns/containers/boxed-lists.html,
  https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography

### 2.3 Typography

| Platform | Rule |
|----------|------|
| GNOME | System font, few sizes and weights, no hard-coded sizes, avoid italics and all-caps. |
| Windows | Segoe UI Variable; ramp caption 12/16, body 14/20, subtitle 20/28, title 28/36; Regular and Semibold; sentence case; minimum 12 px regular or 14 px semibold. |

- **Common denominator:** `system-ui`, four roles (caption, body, heading, title) in `rem`, weights
  400 and 600, sentence case. The current 18 px body is larger than every platform norm (14-15 px).
- Sources: https://developer.gnome.org/hig/guidelines/typography.html,
  https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography

### 2.4 Button hierarchy

| Platform | Rule |
|----------|------|
| GNOME | At most one suggested or destructive button per view; pill style only for the main action in open space. |
| Windows | Dialog: primary, secondary, close; the default button gets the accent and answers Enter. |
| Apple | Alerts: title, optional text, up to three buttons. |

- **Common denominator:** one emphasised action per view or dialog, verb labels, three levels
  (primary, default, destructive), content-width buttons in a footer row. Current full-width stacked
  buttons and `opacity:.5` disabled state are weak.
- Sources: https://developer.gnome.org/hig/patterns/controls/buttons.html,
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/style-classes.html,
  https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs,
  https://developer.apple.com/design/human-interface-guidelines/alerts (partial read)

### 2.5 Forms and validation

| Platform | Rule |
|----------|------|
| Windows | Visible label (placeholder disappears); size field to input; group related fields; errors in the page, not in dialogs. |
| GNOME | Entry error, warning and success classes; affirmative dialog button names a verb and stays disabled until required choices are made. |
| Fluent 2 | Label, hint, inline message saying how to proceed; on failed submit focus moves to a message bar and errors repeat at the fields. |

- **Common denominator:** label above field, hint below, inline error tied with `aria-describedby`
  and `aria-invalid`, never colour alone; validate on submit and blur, not per keystroke.
- Sources: https://learn.microsoft.com/en-us/windows/apps/design/controls/text-box,
  https://developer.gnome.org/hig/patterns/feedback/dialogs.html,
  https://fluent2.microsoft.design/components/web/react/core/field/usage,
  https://fluent2.microsoft.design/components/web/react/core/messagebar/usage,
  https://www.nngroup.com/articles/errors-forms-design-guidelines/ (snippet)

### 2.6 Empty, error and progress states

| Platform | Rule |
|----------|------|
| GNOME | Placeholder (status page): heading with a next step, one-line description, optional action; illustration only for the first main view. Text in the libadwaita StatusPage is not selectable (snippet), a problem for copying error details. |
| Windows | InfoBar: severity error, warning, success, informational; icon and colour backed by text; no rapid flashing; screen readers need close and reopen to re-announce. |
| Apple | No alert at startup; show cached or placeholder data with a quiet label. |

- **Common denominator:** every screen has an empty state (heading, one sentence, one action);
  errors render inline with text, icon and a retry action; `<progress>` when the total is known,
  labelled indeterminate otherwise. No platform progress rule was verified (GNOME page 404).
- Sources: https://developer.gnome.org/hig/patterns/feedback/placeholders.html,
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.StatusPage.html,
  https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar,
  https://developer.apple.com/design/human-interface-guidelines/alerts

### 2.7 Dialogs and confirmations

| Platform | Rule | Button order |
|----------|------|--------------|
| GNOME | Only in response to a user action; 1-3 buttons; prefer undo to confirmation for reversible deletes; Esc cancels; Enter affirmative unless destructive | Cancel first (left) |
| Windows | First line states the issue; specific verbs; always a safe close; one dialog at a time; no dialog for validation | Action left, safe right |
| Apple | Avoid for undoable actions; use for rare, irreversible ones | Default on the right |

- **Common denominator:** dialogs only for irreversible decisions, specific verb on the confirm
  button, safe action focused, Esc cancels, Enter never destructive. Platforms disagree on button
  order; a webview cannot satisfy all, so one order must be chosen (see decision P3).
- Sources: https://developer.gnome.org/hig/patterns/feedback/dialogs.html,
  https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs,
  https://developer.apple.com/design/human-interface-guidelines/alerts

### 2.8 Settings

| Platform | Rule |
|----------|------|
| Windows | Apply immediately, no Save; simple controls; few settings with smart defaults; theme as Light, Dark, Use system; About at the bottom; disabled items explain why. |
| Apple | Good defaults, minimal settings, Cmd+, opens; do not duplicate system settings; keep task options in the task. |
| GNOME | Boxed-list rows with one or two controls; row click activates the control; editable lists have add and per-row remove. Preferences-page rules unverified (404). |

- **Common denominator:** settings as grouped single-column rows (label, description, control),
  instant apply where cheap, follow the OS theme and scale instead of own dark-mode or font settings.
- Sources: https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings,
  https://developer.apple.com/design/human-interface-guidelines/settings,
  https://developer.gnome.org/hig/patterns/containers/boxed-lists.html

### 2.9 Notifications

| Platform | Rule |
|----------|------|
| GNOME | Toast: transient, in-window, short, optional directly related action. Banner: ongoing state (for example offline). System notification only if the user must see it unfocused. |
| Windows | InfoBar for persistent app-wide state, not to confirm a plain action. |

- **Common denominator:** three tiers: banner (persistent state), toast (one-shot, optional Undo),
  OS notification (background events only). Toast needs an `aria-live` region and must not
  auto-dismiss while it holds focus or an action. Auto-dismiss timing is not specified anywhere.
- Sources: https://developer.gnome.org/hig/patterns/feedback/toasts.html,
  https://developer.gnome.org/hig/patterns/feedback/banners.html,
  https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar

### 2.10 Accessibility

| Source | Rule |
|--------|------|
| GNOME | Everything reachable by keyboard; check high contrast and large text; accessible names; screen-reader usable. |
| WCAG 2.2 | Pointer targets at least 24x24 CSS px unless an exception applies. |
| MDN forced-colors | Browser replaces colours and drops shadows; do not design a second theme; use system colour keywords and borders. `AccentColor` may be fixed in some engines. |

- **Common denominator:** semantic HTML, visible `:focus-visible`, `prefers-reduced-motion`,
  `forced-colors` check on Windows, targets 24 px minimum (aim 32-36 px on desktop), status text
  with `role="status"`, no meaning by colour alone. WebView2 (Windows) and WebKitGTK (Linux)
  differ in support and version; verify in both.
- Sources: https://developer.gnome.org/hig/guidelines/accessibility.html,
  https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html,
  https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors,
  https://developer.mozilla.org/en-US/docs/Web/CSS/color_value/system-color

### 2.11 Summary of the platform baseline

| Element | Baseline |
|---------|----------|
| Shell | Left sidebar, Settings last, collapses near 640 px, design floor 1024x600 |
| Style | CSS tokens on system fonts and colours; `color-scheme: light dark` |
| Feedback | One banner slot, one toast region, inline field errors, no error dialogs |
| Dialog | Native `<dialog>` with `showModal()`, only for irreversible actions |
| Settings | In-app page, grouped rows, About at the bottom |
| Accessibility | Checklist per screen, manual run on Orca/WebKitGTK and NVDA/WebView2 |

## 3. Key-as-account apps

Apps split into two families: seed-phrase apps (Session, Element recovery key) make the user write
down a secret; device-bound apps (Briar, Jami, SimpleX, Signal Desktop) keep the key on the device
and say plainly that loss is unrecoverable unless a backup or link exists. Findings come from
shallow clones of the app repositories (string keys and screen files, never built or run) and
official docs. Only Jami, SimpleX, Fractal and KeePassXC/Secrets hold several accounts or
profiles in one desktop app.

### 3.1 Flows per app

| App | Welcome and create/restore | Storage and backup | Loss warning | Several accounts | Remove |
|-----|---------------------------|--------------------|--------------|------------------|--------|
| Session Desktop | Start screen with create and "I have an account"; restore takes recovery password then display name | Seed is the backup, viewable later in settings and hideable; no storage choice; restore recovers about 14 days of messages | FAQ does not word loss explicitly | Single | Choose "device only" or "device and network", then confirm |
| Jami (Qt) | Wizard: create, or "already have an account" (import from device or backup file); advanced fold for SIP | Optional archive password; backup = export archive; persistent backup tip with "why backup?" | Tip says the account exists only on the device and is unrecoverable if lost | Account combo box; "add account" row | Last item of account page; dialog with irrevocable-loss text; confirm checkbox gates Delete |
| SimpleX | Onboarding sequence incl. database passphrase; restore = create a profile, then import database zip | Random passphrase in the OS keystore by default; own passphrase required before export | Lost passphrase unrecoverable; exported DB must not run on two devices | "Your chat profiles" list, "+ Add profile", avatar menu, hidden and muted profiles | Two variants: delete with connections or local data only; separate "Delete database" |
| Briar (Android; no desktop UI) | Nickname then password step; create only | Encrypted with user password, no backup | Strongest wording, at the password step: no recovery if forgotten or app removed | Single | "Forgot password" offers delete and start again; Delete account, Sign out |
| Element / Matrix | Homeserver login (not key-first) | Recovery key made in settings; entered on new device | Warns before removing the last device; reset only if no verified device and no key | Separate sessions | Sign-out shows the recovery-key check first |
| Signal Desktop | Link-only QR; no create | Local data, optional DB key in the OS store | None needed: phone is the root; re-link warns local data will be deleted | Single | "Delete all data": text states local data is cleared and the account/phone untouched |
| Fractal | Greeter: Log In / Create Account; recovery only for encryption keys | Recovery passphrase or key held by user; session in keyring | Reset page lists consequences | Session list with "add new session"; failed sessions removable | Log Out page warns when last session or recovery is missing |
| KeePassXC | New database wizard vs open | User chooses file location; key file kept separately | Key-file helper text: keep it secret, never lose it | Tabs | Remove key file is an explicit button |
| GNOME Secrets | Welcome with "New Safe" and open file | File chooser; recent list | Little explicit wording | Recent list | No deletion flow |

Sources: https://getsession.org/faq, https://briarproject.org/manual/,
https://simplex.chat/docs/guide/chat-profiles.html,
https://simplex.chat/docs/guide/managing-data.html,
https://github.com/simplex-chat/simplex-chat/blob/stable/docs/guide/chat-profiles.md
(Session, Jami, Element, Signal, Fractal, KeePassXC and Secrets: source files and string keys in the
shallow clones; Briar manual and Signal support pages failed to load, see section 10).

### 3.2 Cross-app patterns

1. **Two-door welcome.** Session, Jami, Fractal and Secrets show a primary create action beside an
   equally visible "I already have one".
2. **Loss warning at the moment of secret creation**, in consequence wording: where it lives, what
   happens if lost, what to do. Seen at the Briar password step, Jami's persistent backup tip,
   SimpleX docs, and KeePassXC key-file helper text.
3. **Backup is one named explicit action** (Jami export archive, SimpleX export database, Session
   view recovery password, Element create recovery key), reachable from settings and re-checked
   before destructive actions.
4. **Destructive friction scales with irreversibility and names the scope** (local only vs also
   network; what is not deleted). Fractal and Element warn on the last session or device.
5. **Multi-account = one switcher that also hosts "add"** (Jami combo box, SimpleX avatar menu);
   secondary actions (hide, mute, delete) sit in context menus or per-account settings.

### 3.3 Lessons for dyapp

| Topic | Lesson |
|-------|--------|
| Setup | Make restore a first-class peer of create (matches the Briar gap in reverse: Briar has none and users on a new machine are stuck). dyapp's import rule "inspect key, derive Peer ID, confirm before activation" is already a good reuse point. |
| Storage choice | Keep the pre-create choice (system store vs key file). SimpleX defaults to the keystore and makes export explicit; KeePassXC and Secrets make the user aware where the secret lives. |
| Picker | Jami and SimpleX put "add" in the same list as the accounts; rows need distinguishing detail because Peer IDs look alike. |
| Recovery | Say what failed, what it means for the key, then the safe next step (Fractal, Element). Never imply a new identity replaces the old one. |
| Remove | State the scope in one sentence (Signal, Session): this computer only; contacts, network and published profile are not erased. SimpleX's "export before delete" needs an export feature that is only planned. |

## 4. Network entry and connection status

dyapp differs from most references: it has no working default seed, so the seed list is the
primary field, not an advanced one. What can be hidden is the optional network name and the pin
syntax help.

### 4.1 Per-app patterns

| App | Default vs advanced | Progress and failure | After setup |
|-----|--------------------|----------------------|-------------|
| Tor Browser | One Connect button; "Configure Connection" is a secondary route for bridges and proxy | Progress bar; on stall, Connection Assist (bridge by region) and a connection test with success or failure | "Always connect automatically" skips the screen on later launches; post-connect display undocumented |
| Syncthing | Add a device by device ID; address defaults to `dynamic`; discovery and listen addresses are advanced with `default` values | No wizard; per-device state in the device list; connection type (direct or relay) visible, so a degraded path is not an error | Main window is the status surface (folders and devices lists with aggregate rates) |
| Jami | Bootstrap host has a default; change under Account, Advanced, OpenDHT; several hosts in one semicolon-joined field (snippet) | Not documented in sources reached | Registration state in account list (snippet) |
| qTox | Built-in node list; custom nodes only by editing a JSON file; custom node needs its DHT key | No progress UI; silent retry after a countdown; issue shows a reachable custom node failing with no explanation | Self-status dot (green, yellow, red, gray) doubles as connectivity |
| Briar | Settings, Networks: automatic by location; Tor, Tor with bridges, or none | Pending contact retries, then after 48 hours shows "adding contact has failed" with a remedy | Status display undocumented |
| SimpleX | Preset servers work; custom servers by pasting one address with embedded fingerprint; presets keepable, removable or disableable | Server test feature not covered by the fetched page | Settings explain that a change only affects new contacts |
| Transmission, qBittorrent | Listening port is a plain preference with a default | Port test only on request; "closed" distinguished from "unknown"; qBittorrent shows "DHT: 0 nodes" briefly after start | qBittorrent status bar: icon (connected, firewalled, offline), DHT count, speeds, tooltip; "firewalled" is a warning, not an error (third-party sources) |
| Element | Default homeserver; "Edit homeserver" reveals a URL field; deployments can disable it | Validates on Continue; reported failures: invalid URL doing nothing; vague "couldn't reach this homeserver" hiding the cause | n/a |

Sources: https://support.torproject.org/running-tor-browser/,
https://docs.syncthing.net/intro/gui.html, https://docs.syncthing.net/users/config.html,
https://docs.syncthing.net/users/faq.html, https://docs.jami.net/en_US/user/lan-only.html (snippet),
https://docs.jami.net/en_US/user/faq.html (snippet),
https://forum.jami.net/t/how-to-specify-a-bootstrap-server-to-jamid/5381 (snippet),
https://github.com/qTox/qTox/blob/master/doc/user_manual_en.md,
https://qtox.github.io/doxygen/core_8cpp_source.html, https://github.com/qTox/qTox/issues/5794,
https://briarproject.org/manual/, https://simplex.chat/docs/server.html,
https://trac.transmissionbt.com/ticket/1257, https://forum.qbittorrent.org/viewtopic.php?t=3229,
https://natchecker.com/blog/qbittorrent-connection-status-firewalled (third-party),
https://github.com/element-hq/element-web/issues/10274 (snippet),
https://github.com/element-hq/element-x-android/issues/4556 (snippet),
https://github.com/element-hq/element-web/blob/develop/docs/config.md (snippet)

### 4.2 Takeaways

- Failure is best shown per row plus one summary line, with distinct causes; generic "couldn't
  reach" messages draw complaints.
- Cancel sits next to the running action; retry is the same primary button relabelled.
- Give long attempts an explicit end and remedy (Briar 48 hours) rather than a spinner.
- Run reachability tests on request, and say "unknown" when unsure (Transmission).
- After setup, status lives in a small always-visible indicator with a click-through to detail,
  not on the setup form. Separate warning (some seeds unreachable) from error (none reachable).
- Show degraded-but-working states honestly (Syncthing relay).
- Never fail silently; keep technical detail available (Element).

## 5. Profile editing, publish confirmation and search

dyapp publishes explicitly (proof of work, signed readback, public and copyable after deletion),
which is stricter than any reference. Current gaps: no persistent state label (draft, unsaved,
published), no unsaved-change indicator, and a raw two-letter country input on search while the
profile uses a select.

### 5.1 Patterns per source

| Source | Edit vs preview | Visibility and consent | Notes |
|--------|-----------------|------------------------|-------|
| Hinge | "View" control inside the editor shows the profile as others see it | n/a | Fixed slots with minimum counts gate a complete profile |
| Bumble | Preview is the default Me view; explicit "Edit profile" | Age comes from a locked date of birth; showing age is separate | Advanced filters are premium (snippet) |
| Tinder | n/a | One toggle "Show me on Tinder" beside search preferences; off hides from new people only | Ranges may be exceeded when candidates run out |
| OkCupid | n/a | n/a | Separate Match Search removed because its preferences diverged from the profile; Discover shows 5 people per stack |
| Mastodon | Edit form with Save; leaving without saving loses edits (snippet) | Directory listing is an explicit flag, off by default | Few extra fields (four) |
| Signal | Profile opened from avatar; first name required | Help page states who receives the profile | State the audience in plain words beside the form |
| libadwaita | StatusPage: icon, title, description, one action; spinner variant | Banner for persistent state, toast for transient confirmation, alert dialog for confirmation | Type-to-search entry above the list; no-results separate from empty list |
| Fluent 2 | Message bar on failed submit with an action | n/a | Do not mark every field required; state the rule once |
| Material 3 | Filter chips under a search field, never alone | n/a | With 4-5 filters chips add little over labelled fields |

Sources: https://help.hinge.co/hc/en-us/articles/360011053094-How-do-I-edit-my-profile,
https://help.hinge.co/hc/en-us/articles/36311352171539-How-do-I-edit-my-Prompts,
https://support.bumble.com/hc/en-us/articles/31602664871453-Adding-profile-prompts,
https://apps.apple.com/us/app/bumble-app/id930441707 (snippet),
https://www.help.tinder.com/hc/en-gb/articles/115003340963-Discovery-Settings,
https://okcupid-app.zendesk.com/hc/en-us/articles/23110176786587-Finding-searching-and-adjusting-who-you-see,
https://docs.joinmastodon.org/user/profile/, https://docs.joinmastodon.org/user/discoverability/,
https://github.com/mastodon/mastodon/discussions/17218,
https://fedi.tips/how-do-i-customise-my-profile-how-do-i-upload-a-profile-banner-how-do-i-verify-my-account/ (snippet),
https://support.signal.org/hc/en-us/articles/360007059952-Edit-my-Profile,
https://support.signal.org/hc/en-us/articles/360007459591-Signal-Profiles-and-Message-Requests,
https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.StatusPage.html,
https://lobehub.com/skills/neversight-skills_feed-designing-gnome-ui (third party, lower trust),
https://fluent2.microsoft.design/components/web/react/core/field/usage,
https://fluent2.microsoft.design/components/web/react/core/messagebar/usage,
https://m3.material.io/components/chips/guidelines, https://m2.material.io/components/chips,
https://www.nngroup.com/articles/errors-forms-design-guidelines/ (snippet).
The Bumble editing support page and one other URL were redacted in the source report and are
omitted here.

### 5.2 Takeaways

- No dating app has a separate publish step; edits go live on save. Preview in dyapp must
  therefore be a mandatory step, not an optional view.
- Visibility beyond "technically public" is a named, default-off switch (Mastodon, Tinder).
- Dating apps use loose search preferences; dyapp must stay strict and honest that results are a
  sample.
- One place for filters, no hidden second set (OkCupid); short batches fit "load another sample".
- Empty and error states share one shape (StatusPage, message bar with an action).
- Error text should be selectable so users can copy details.

## 6. Settings structure and destructive actions

### 6.1 Settings structure per app

| App | Navigation | Per-account vs app-wide | Remove location | Confirmation | Recoverable |
|-----|------------|------------------------|-----------------|--------------|-------------|
| GNOME | Sidebar of panels; preferences dialog with pages and groups | Online Accounts panel | Account row | Dialog, verb button | By policy |
| Windows | Sidebar (about 11 categories), breadcrumb, hero control, explicit Advanced page | Accounts category | Per page | Dialog (not studied) | Varies |
| Element | One modal with left tab list; Labs and About last | General tab holds profile and Deactivate (last) | End of General | Permanence warning (snippet) | No |
| Signal Desktop | Modal sections | Single account | General, last | Scope sentence | No (local) |
| Thunderbird | Tree with accounts as nodes; app settings in a separate window | Structurally obvious | "Account Actions" menu at bottom of left pane | Dialog; local files remain (community answers) | Files remain |
| Firefox | Separate Profile Manager window | List | Remove | Two-way choice: delete files or not (keep is recommended) | Optional |
| KeePassXC | Application dialog plus per-database dialog | Per database | Context | Scales with recoverability; recycle-bin move needs none, permanent delete asks | Recycle bin |

Sources: https://developer.gnome.org/hig/patterns/feedback/dialogs.html,
https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/ (snippet),
https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.2/class.Toast.html (snippet),
https://help.gnome.org/users/gnome-help/stable/accounts-remove.html.hr,
https://support.microsoft.com/en-us/accessibility/windows/understand-and-explore-windows-settings (snippet),
https://docs.element.io/latest/element-support/element-webdesktop-client-settings/element-settings/,
https://docs.element.io/latest/element-support/element-webdesktop-client-settings/sessions/ (snippet),
https://github.com/signalapp/Signal-Desktop/commit/a8722716c14343bcf8459d71e37db7c048aa5cc0,
https://github.com/signalapp/Signal-Desktop/issues/3728,
https://support.signal.org/hc/en-us/articles/360007321111-Unlinking-devices (snippet),
https://support.mozilla.org/en-US/kb/configuration-options-accounts,
https://support.mozilla.org/en-US/questions/1036973 (snippet),
https://support.mozilla.org/en-US/kb/profile-manager-create-remove-switch-firefox-profiles (snippet),
https://bugzilla.mozilla.org/show_bug.cgi?id=1442200,
https://keepassxc.org/docs/KeePassXC_UserGuide (TOC only); KeePassXC settings, config and window
sources were read in a shallow clone.

### 6.2 Takeaways

- Every mature app separates app-wide settings from per-account settings; all stay under about 5
  pages.
- Account removal sits at the bottom or edge of the per-account area, away from routine fields,
  and is never primary-styled.
- Confirmation strength follows recoverability (KeePassXC, Firefox). A dyapp key removal is
  unrecoverable, so it belongs in the heavy group.
- Undo is best when deletion can truly be deferred (GNOME Toast docs: delete after the toast
  closes). Deferring key destruction keeps the key on disk, which contradicts "removed means
  destroyed" (hypothesis, not checked against the implementation).
- Firefox bug lesson: test the destructive path against exactly what the label promises. Firefox
  also refuses to remove the in-use profile.
- Backup is shown where the secret lives (Element Encryption tab, KeePassXC opt-in backup) and as a
  consequence in the deletion dialog, not as a separate nag.
- Windows' hero status line on top of Network and separate Advanced page.

## 7. Screenshot index

Links only; nothing is stored in the repo. HTTP status was checked on 2026-10-28 (all 200). Image
content was not opened: "step shown" comes from the Flathub caption, image `alt`, or surrounding doc
text. Rows marked **(caption only)** have no more than that; a human should open them before a
design decision rests on them. Flathub links are `_orig.png` files from
`https://flathub.org/api/v2/appstream/<id>`; app pages are `https://flathub.org/apps/<id>`. Version
or date is the app version (Flathub) or the file-name date (docs).

### 7.1 First run and welcome

| App | Step | Link | Version |
|-----|------|------|---------|
| Tor Browser | Start window with Connect | https://support.torproject.org/tor-browser/getting-started/first-launch/connect.png | current |
| Tor Browser | Language choice (page text) | https://tb-manual.torproject.org/running-tor-browser/ | undated |
| Syncthing docs | First start (caption only, stale) | https://docs.syncthing.net/_images/gs1.png | v1.7.1, 2020 |
| KeePassXC | Unlock database | https://dl.flathub.org/media/org/keepassxc/KeePassXC.desktop/8703bd4e659abcb1dac1d6646881bb00/screenshots/image-2_orig.png | 2.8.0 |
| GNOME Secrets | Safe unlocking (caption only) | https://dl.flathub.org/media/org/gnome/World.Secrets/c3aeb38516791b22b131a795233d2f0f/screenshots/image-2_orig.png | 14.0 |

### 7.2 Create vs restore

| App | Step | Link | Version |
|-----|------|------|---------|
| GNOME Secrets | Safe creation (caption only) | https://dl.flathub.org/media/org/gnome/World.Secrets/c3aeb38516791b22b131a795233d2f0f/screenshots/image-3_orig.png | 14.0 |
| GNOME Secrets | App page | https://flathub.org/apps/org.gnome.World.Secrets | 14.0 |
| SimpleX docs | Creating additional profiles | https://simplex.chat/blog/images/20230204-profiles2.png | 2023-02 |

### 7.3 Backup and loss warning

| App | Step | Link | Version |
|-----|------|------|---------|
| SimpleX docs | Database passphrase | https://simplex.chat/blog/images/20220928-passphrase.png | 2022-09, old UI |
| SimpleX docs | Delete files and media | https://simplex.chat/blog/images/20220928-files-media.png | 2022-09 |

### 7.4 Account switch and add

| App | Step | Link | Version |
|-----|------|------|---------|
| SimpleX docs | Switching profiles | https://simplex.chat/blog/images/20230204-profiles1.png | 2023-02 |
| SimpleX docs | Add profile | https://simplex.chat/blog/images/20230204-profiles2.png | 2023-02 |
| SimpleX docs | Hidden profiles | https://simplex.chat/blog/images/20230328-hidden-profiles1.png | 2023-03 |
| SimpleX docs | Incognito mode | https://simplex.chat/blog/images/20220901-incognito1.png | 2022-09 |
| Element docs | Sessions list (sessions, not accounts) | https://docs.element.io/latest/assets/images/bookstack/image-1690992436887.png | 2023-08 |

### 7.5 Remove and delete confirmation

| App | Step | Link | Version |
|-----|------|------|---------|
| Element docs | Sign out a session (alt text only) | https://docs.element.io/latest/assets/images/bookstack/image-1690992891681.png | 2023-08 |
| SimpleX docs | Delete files and media (not profile removal) | https://simplex.chat/blog/images/20220928-files-media.png | 2022-09 |

### 7.6 Network entry and connection status

| App | Step | Link | Version |
|-----|------|------|---------|
| Tor Browser | Connection Assist, automatic | https://support.torproject.org/tor-browser/getting-started/first-launch/connection-assist-auto.png | current |
| Tor Browser | Connection Assist, select | https://support.torproject.org/tor-browser/getting-started/first-launch/connection-assist-select.png | current |
| Tor Browser | Configure connection | https://support.torproject.org/tor-browser/getting-started/first-launch/configure.png | current |
| Tor Browser | Test success | https://support.torproject.org/tor-browser/getting-started/first-launch/connection-test-success.png | current |
| Tor Browser | Test failure | https://support.torproject.org/tor-browser/getting-started/first-launch/connection-test-failure.png | current |
| Tor Browser | Bridge choice | https://support.torproject.org/tor-browser/getting-started/first-launch/pluggable-transport.png | current |
| Tor Browser | Proxy entry | https://support.torproject.org/tor-browser/getting-started/first-launch/proxy.png | current |
| Tor Browser | Connect automatically | https://support.torproject.org/tor-browser/getting-started/first-launch/connect-automatically.png | current |
| SimpleX docs | Network and servers | https://simplex.chat/blog/images/20230204-transport.png | 2023-02 |
| SimpleX docs | Server list, add server | https://simplex.chat/blog/images/20221206-server1.png | 2022-12 |
| SimpleX docs | .onion hosts | https://simplex.chat/blog/images/20220901-onion1.png | 2022-09 |
| Syncthing docs | Add remote device (step inferred from text) | https://docs.syncthing.net/_images/gs2.png, https://docs.syncthing.net/_images/gs3.png | v1.7.1, 2020 |
| Syncthing Tray | Tray with connection status | https://dl.flathub.org/media/io/github/martchus.syncthingtray/92bd3d03e5356e34f12dd844079d8dfc/screenshots/image-1_orig.png | 2.1.8 |
| qBittorrent | Main window, status bar (caption only) | https://dl.flathub.org/media/org/qbittorrent/qBittorrent/c5c5043daf205a37d36fe5bcd82495b2/screenshots/image-1_orig.png | 5.3.0 |
| Transmission | Main window | https://dl.flathub.org/media/com/transmissionbt/Transmission/555db5eb6718f72f8880ae8c4a8773fe/screenshots/image-1_orig.png | 4.1.4 |

### 7.7 Profile edit and publish

| App | Step | Link | Version |
|-----|------|------|---------|
| Mastodon docs | Profile card | https://docs.joinmastodon.org/assets/profile-card.png | undated |
| Mastodon docs | Automated flag | https://docs.joinmastodon.org/assets/bot-flag.png | undated |
| Tuba | Profile view | https://dl.flathub.org/media/dev/geopjr/Tuba/150f61fcc01e79969370d454fbe39642/screenshots/image-3_orig.png | 0.11.1 |
| Tuba | Composer | https://dl.flathub.org/media/dev/geopjr/Tuba/150f61fcc01e79969370d454fbe39642/screenshots/image-2_orig.png | 0.11.1 |
| Tuba | Schedule Post dialog | https://dl.flathub.org/media/dev/geopjr/Tuba/150f61fcc01e79969370d454fbe39642/screenshots/image-5_orig.png | 0.11.1 |
| SimpleX docs | Contact address creation | https://simplex.chat/blog/images/20221108-address1.png | 2022-11 |

Edit-form screenshots are thin: Mastodon shows the result, not the form.

### 7.8 Search and filters

| App | Step | Link | Version |
|-----|------|------|---------|
| Tuba | Search view | https://dl.flathub.org/media/dev/geopjr/Tuba/150f61fcc01e79969370d454fbe39642/screenshots/image-7_orig.png | 0.11.1 |
| qBittorrent | Search engine tab | https://dl.flathub.org/media/org/qbittorrent/qBittorrent/c5c5043daf205a37d36fe5bcd82495b2/screenshots/image-4_orig.png | 5.3.0 |
| qBittorrent | Filter sidebar (caption only) | https://dl.flathub.org/media/org/qbittorrent/qBittorrent/c5c5043daf205a37d36fe5bcd82495b2/screenshots/image-2_orig.png | 5.3.0 |

### 7.9 Settings

| App | Step | Link | Version |
|-----|------|------|---------|
| qBittorrent | Options dialog with category sidebar | https://dl.flathub.org/media/org/qbittorrent/qBittorrent/c5c5043daf205a37d36fe5bcd82495b2/screenshots/image-3_orig.png | 5.3.0 |
| Syncthing Tray | Settings dialog | https://dl.flathub.org/media/io/github/martchus.syncthingtray/92bd3d03e5356e34f12dd844079d8dfc/screenshots/image-3_orig.png | 2.1.8 |
| Tor Browser Launcher | Settings window | https://dl.flathub.org/media/org/torproject/torbrowser-launcher/8bbf3d7c3768830e11a10b6b47366829/screenshots/image-1_orig.png | 0.3.9 |
| SimpleX docs | App settings guide | https://simplex.chat/docs/guide/app-settings.html | undated |
| Element docs | Settings, Sessions page | https://docs.element.io/latest/assets/images/bookstack/image-1690992096339.png | 2023-08 |
| Firefox, Thunderbird | Main windows only, no captions | https://flathub.org/apps/org.mozilla.firefox, https://flathub.org/apps/org.mozilla.Thunderbird | 159.0 / 140.10.2esr |

### 7.10 App pages queried

Flathub pages (17 listings, 41 images, all HTTP 200): Element https://flathub.org/apps/im.riot.Riot;
Fractal https://flathub.org/apps/org.gnome.Fractal; Jami https://flathub.org/apps/net.jami.Jami
(marketing shots); Signal https://flathub.org/apps/org.signal.Signal; SimpleX
https://flathub.org/apps/chat.simplex.simplex (one shot, no caption); KeePassXC
https://flathub.org/apps/org.keepassxc.KeePassXC; qTox https://flathub.org/apps/io.github.qtox.qTox;
Session https://flathub.org/apps/network.loki.Session (one shot, no caption); Briar
https://flathub.org/apps/org.briarproject.Briar. Molly and GNOME Settings ids returned 404.

### 7.11 Gaps

Flows with fewer than two apps with public screenshots:

| Flow | Gap |
|------|-----|
| First run, welcome | One current set (Tor Browser); Syncthing is stale; Session, Jami, Signal, Briar, Element have none. Jami docs https://docs.jami.net/user/create-jami-account.html have steps but no images in the static HTML |
| Create vs restore | No two-door screen anywhere; only Secrets and SimpleX show creation, neither shows restore |
| Backup and loss warning | SimpleX passphrase only (2022 image); Briar manual did not load, Session recovery screens not public |
| Account switch and add | SimpleX only; Jami switcher and Fractal session list have no official shot |
| Remove and delete | None confirmed; Mozilla and Signal support pages had no static `<img>` tags (may load by script) |

Date caveat: SimpleX guide images date from 2022-2023 and Syncthing's from 2020. Four of nine flows
(network, profile, search, settings) have two or more apps. The five gap flows rely on the text
findings in sections 3 and 6.

**Source tiers (recommendation R1, section 8):** tier 1 official docs and help pages (Tor support,
SimpleX guide, Element docs, Mastodon docs) for flow steps; tier 2 Flathub screenshots for
settings, search, profile and main-window layout.

## 8. Candidate decisions

All entries are candidates; none is accepted. Cost: S small (copy, markup, one component), M medium
(new component with state wiring or core events), L large (new design or format). The prefix groups
the topic: P platform, K key-as-account, N network, F profile and search, S settings, R references.

### 8.1 Overview

| ID | Decision | Cost | Depends on |
|----|----------|------|------------|
| P1 | Sidebar shell collapsing near 640 px | M | none |
| P2 | CSS design tokens on system fonts and colours | S | none |
| P3 | Button hierarchy and native `<dialog>` with one button order | S-M | P2 |
| P4 | Feedback tiers: banner, toast, inline error | M | P2 |
| P5 | Settings as an in-app page; Apply only where validation is needed | M | P1, S1 |
| P6 | Accessibility baseline and manual checklist | S (M with manual runs) | P2 |
| K1 | Two-door welcome (create first, restore second) | M | none |
| K2 | Storage choice wording and order | S | none |
| K3 | Ready screen with backup nudge | S-M | K2 |
| K4 | Header identity switcher with "add identity" row | M | P1 |
| K5 | Recovery screen: Retry, Restore, Remove | S | K1 |
| K6 | Remove screen scope line | S | none |
| N1 | Network form: seeds primary, name behind disclosure | S | none |
| N2 | Check and cancel flow with bounded timeout | S | none |
| N3 | Persistent connection status indicator | M | P4 |
| F1 | Profile status line and unsaved-change indication | S | none |
| F2 | Publish confirmation: two separate unticked checkboxes | S | F1 |
| F3 | Search form and results with sample notice | M | none |
| S1 | Settings grouped in four sections | M | P1 |
| S2 | Tiered removal with typed confirmation on last copy | S | K6 |
| S3 | Backup warnings at three touchpoints, no nag | S | K2 |
| S4 | Node options under an Advanced disclosure | S | N1, S1 |
| R1 | Two-tier reference sources, no local screenshot copies | S | none |

### 8.2 Platform decisions

#### P1. Navigation shell

- **Recommendation:** one left sidebar (icon and label, Settings pinned last) that becomes an icon
  rail at 641-1007 px and an off-canvas menu at 640 px and below.
- **Pros:** all platforms endorse it for 4+ areas; one pattern for GNOME, KDE and Windows; later
  maps to a macOS sidebar and an M3 rail.
- **Cons:** costs width at the 1024x600 floor; collapse and focus logic must be built by hand.
- **Alternatives:** top tabs (simple, but overflow and poor GNOME fit); linear wizard as today (fits
  first run, poor for returning users); hamburger only (minimal, hides destinations).
- **Cost:** M. See open question Q7 on how this interacts with the header switcher (K4) and the
  in-Settings list (S1).

#### P2. Design tokens

- **Recommendation:** CSS custom properties for a 4 px spacing scale, four text roles, radii and
  semantic colours; `system-ui`, `color-scheme: light dark`, `prefers-color-scheme`, `AccentColor`
  with a fixed fallback.
- **Pros:** one stylesheet for all desktops; automatic dark mode and text scale; no bundled fonts or
  icons, so MIT-safe.
- **Cons:** no pixel-native look; `AccentColor` not guaranteed across engines; several palettes to
  eyeball.
- **Alternatives:** per-desktop themes detected at runtime (closest look, large test matrix,
  brittle detection); a UI library such as Fluent web components (ready widgets, new dependency and
  licence review, Windows flavour on GNOME); browser defaults (zero work, today's oversized look).
- **Cost:** S, one CSS file replacing the inline style block.

#### P3. Button hierarchy and dialog pattern

- **Recommendation:** three levels, one primary per view; content-width buttons in a footer row;
  native `<dialog>` for confirmations with specific verbs, Esc cancels, Enter never triggers a
  destructive action; one button order for all desktops (the report assumes Cancel left, confirm
  right).
- **Pros:** matches shared rules; `<dialog>` gives focus trap and Esc; one order avoids per-OS
  branching.
- **Cons:** Windows users may expect the action on the left.
- **Alternatives:** OS-dependent order (native feel, detection and double tests); Tauri native
  dialog plugin (real look, little control over text, hard to test in shared UI); inline expand
  (no modal, easy to miss for key deletion).
- **Cost:** S-M. Conflict: publish (F2) wants no modal and removal (S2) wants a dedicated view;
  see Q6.

#### P4. Feedback tiers

- **Recommendation:** one persistent banner slot, one toast region (`aria-live`) with Undo where
  reversible, inline field errors, no error dialogs; OS notification only when hidden to tray.
- **Pros:** maps to GNOME banners and toasts and the WinUI InfoBar; fewer modal interruptions; fits
  existing `role="status"` paragraphs.
- **Cons:** three components to build and test with screen readers; auto-dismiss timing unspecified.
- **Alternatives:** status paragraphs only as today (errors scroll away, no hierarchy); modal for
  all errors (never missed, disruptive); OS notifications for all (permission and per-OS
  differences, spammy).
- **Cost:** M.

#### P5. Settings behaviour

- **Recommendation:** Settings page last in the sidebar, single column, grouped rows, About at the
  bottom; follow the system theme; instant apply except network and node fields, which keep an
  explicit Apply with a visible pending state.
- **Pros:** matches WinUI and Apple; avoids invalid half-states for network parameters.
- **Cons:** two behaviours on one page can confuse unless the pending state is clear.
- **Alternatives:** all instant (simplest, unsafe for settings that need a connection check);
  separate preferences window (GNOME and macOS convention, multi-window Tauri cost); Save and
  Cancel for the whole page (one rule, dated).
- **Cost:** M.

#### P6. Accessibility baseline

- **Recommendation:** semantic HTML, `:focus-visible`, 24 px minimum targets (aim 32 px), reduced
  motion, a `forced-colors` pass on Windows, visible labels, no colour-only status; a manual
  keyboard and screen-reader checklist per screen.
- **Pros:** cheap now, expensive to retrofit; WCAG 2.2 gives a measurable bar.
- **Cons:** screen-reader behaviour differs per engine (Orca with WebKitGTK, NVDA or Narrator with
  WebView2); cannot be fully automated.
- **Alternatives:** defer past v0.0.1 (speed now, dialog and focus structure is set early so
  retrofit cost is high); automated axe-style checks only (CI-friendly, catches a minority).
- **Cost:** S for CSS and markup; M with manual runs on three engines.

### 8.3 Key-as-account decisions

#### K1. Two-door welcome

- **Recommendation:** first run shows "Create identity" (primary) and "Restore from backup"
  (secondary) before the storage choice. Restore opens a picker for a copied `profiles/<slot>/`
  folder and shows the derived Peer ID for confirmation before activation.
- **Pros:** the pattern users know from Session, Jami and Fractal; restore becomes discoverable;
  reuses the "inspect key, derive Peer ID before activation" rule.
- **Cons:** extra screen and failure modes (bad folder, UUID collision, system-store slot with no
  credential); needs tests and copy.
- **Alternatives:** create-only first screen with import in Settings (smaller, but a returning user
  on a new machine finds nothing); advanced fold for import as in Jami (less clutter, but restore is
  common enough); link-only onboarding as in Signal Desktop (not applicable, no phone root).
- **Cost:** M. Conflict with S3: the settings report keeps export and restore "planned and
  disabled"; see Q5.

#### K2. Storage choice screen

- **Recommendation:** keep the pre-create choice, ordered "Protected by your system" (recommended)
  then "Key file in app folder", each with one line on what is protected and one on what backup
  means; show the exact path only after choosing file storage; no password step in v0.0.1.
- **Pros:** consistent with SimpleX and KeePassXC; already implemented; disabled option shows a
  reason.
- **Cons:** file storage stays unencrypted at rest; users may skip the backup line; Windows ACL
  limit remains.
- **Alternatives:** optional passphrase for file storage (stronger, adds a lost-passphrase failure
  mode and needs crypto design; L); hide the choice and pick automatically (simple, contradicts the
  rule that no backend is selected silently); single page with advanced fold (less load, hides a
  security decision).
- **Cost:** S.

#### K3. Ready screen and backup nudge

- **Recommendation:** after creation show the Peer ID with a copy button and one short warning
  (where the key is, what happens if lost) with a "Back up now" action (file storage: open folder or
  show path; system storage: explain no file backup exists and export is planned). Keep a
  persistent, dismissible "Not backed up" row in Settings until acknowledged.
- **Pros:** follows patterns 2 and 3; non-blocking; reuses the path display.
- **Cons:** for system storage the nudge can only say "not available"; the acknowledged flag is
  local and self-reported.
- **Alternatives:** mandatory write-down step as Session (strongest, but dyapp has no mnemonic; L);
  no nudge, docs only (zero cost, but loss is the main risk); first-launch modal as Briar (easily
  dismissed and forgotten).
- **Cost:** S-M. Conflict with S3 (no dismissable nag): see Q4.

#### K4. Picker and switching

- **Recommendation:** keep the full-window picker at startup when more than one identity exists;
  in the running app use a header switcher (short Peer ID, list, "Add identity" last row), rows with
  Peer ID, storage mode and a recovery badge; switching always goes through the existing
  stop-network step.
- **Pros:** familiar; add and switch in one place; incomplete or missing slots are visible.
- **Cons:** header space on small windows; near-identical Peer IDs are hard to tell apart without
  labels.
- **Alternatives:** Settings-only "Switch or add" as today (no new UI, two clicks deep); avatar rail
  (fast, costs width and needs a visual identity system); optional local label per identity (helps
  recognition, must not leak to the network, adds a slot field; S).
- **Cost:** M. Conflict with S1: see Q1.

#### K5. Recovery screen

- **Recommendation:** Retry primary, "Restore from backup" secondary, "Remove this identity"
  tertiary. Wording: what failed, what it means for the key, the safe next step; never imply a new
  identity replaces the old one.
- **Pros:** consistent with the no-regeneration rule; the three outcomes are the real choices.
- **Cons:** more buttons on an error screen; Restore applies only to file slots and must be hidden
  or explained for system slots.
- **Alternatives:** Retry and Remove only as today (no path for a user with a backup); wizard-style
  troubleshooting (friendlier, long).
- **Cost:** S.

#### K6. Remove screen scope line

- **Recommendation:** keep Peer ID, storage, path, last-copy facts and the tick box; add a scope
  line: removes this identity and its local data from this computer; contacts, the network and other
  devices are unchanged; a published profile is not erased. Emphasise "last copy" only when true.
- **Pros:** close to Jami; removes the Session-style network ambiguity.
- **Cons:** more text; a ticked box can still be clicked by mistake.
- **Alternatives:** typed confirmation always (stronger, annoying, bad for missing-key slots; see
  S2); two-step with export prompt first (safer, export is only planned); plain confirm as Signal
  (weak for key loss).
- **Cost:** S.

### 8.4 Network decisions

#### N1. Network form

- **Recommendation:** seed textarea (one per line) and Check always visible; network name under a
  native `<details>` "Network name (optional)", open when a name is saved or a mismatch occurs; one
  helper line and an example placeholder for `host:port`, `[IPv6]:port` and optional peer-ID pin;
  validate per row on blur and on Check, naming the row.
- **Pros:** no default seeds exist, so seeds stay primary; `<details>` needs no JS; per-row
  messages avoid Element's vague failure.
- **Cons:** a collapsed name is easy to miss for private-network users; pin syntax stays in help
  text.
- **Alternatives:** everything visible as today (no discoverability risk, implies the name is
  required); two-step Tor-style wizard (clear, but step one would be empty); structured rows with
  separate host, port and pin (per-field validation, heavier UI, worse paste).
- **Cost:** S. Conflict with S4 on what sits under the disclosure: see Q2.

#### N2. Check and cancel flow

- **Recommendation:** one primary button: "Check connection" becomes disabled "Checking..." with
  Cancel beside it; summary in the `role="status"` line plus ordered per-row results (reached,
  unreachable, wrong network or pin, malformed) with short reasons; after failure the button reads
  "Check again" and input is kept; editing clears Continue; bounded overall timeout stated in the
  message; Continue stays disabled until a check of the unchanged draft succeeds.
- **Pros:** consistent with Tor, Briar and Transmission; reuses existing markup; Continue gating
  prevents activating unverified settings.
- **Cons:** a row can honestly only say "no reply" (NAT or firewall), so messages hedge; timeout
  needs tuning; focus handling for two-button states.
- **Alternatives:** auto-check on typing (instant, spams unknown nodes, against the Transmission
  guidance); indeterminate bar with no per-row output (simple, loses which row failed); "continue
  anyway" after failure (works offline, contradicts the screens design).
- **Cost:** S.

#### N3. Persistent status indicator

- **Recommendation:** after setup, a compact status in a fixed footer or header strip: dot plus text
  (Connected, Connecting..., Offline, Needs attention), never colour only. Click opens a panel with
  active seeds, last result, "Check again" and "Edit network". Distinguish warning from error. The
  form itself does not stay on screen.
- **Pros:** qBittorrent and qTox keep this glanceable form; one entry point for diagnosis and edit;
  accessible.
- **Cons:** needs a status event channel from the Rust core to the UI; more states to translate;
  footer space on small windows; "connected" is undefined (any seed or a quorum).
- **Alternatives:** Settings-only status (nothing to build, users learn of outage when actions
  fail); badge only on problems (quiet, no reassurance); Syncthing-style peer table (too much).
- **Cost:** M. Conflict with S1 (identity-scoped Network section with a status hero line): see Q1.

### 8.5 Profile and search decisions

#### F1. Profile states and unsaved edits

- **Recommendation:** one page with one status line showing exactly one of: "Draft, not
  published", "Unsaved changes", "Published (matches saved draft)", "Published, draft differs". Keep
  Save and Preview; compute "unsaved" by comparing the serialised form with the last saved draft;
  Save enabled only when different; Preview and Publish only when saved; in-page confirm (not
  `window.confirm`, unreliable in webviews) when leaving with unsaved changes; Publish hidden until
  the preview of the current saved draft was opened; birth parts in a fieldset with one hint on
  which precision becomes public.
- **Pros:** answers "is this live?" at a glance; no lost edits; fits the rule that edits clear
  confirmations.
- **Cons:** four states mean four strings and tests; "published, draft differs" needs a comparison
  with the signed readback; forced preview adds a click per republish.
- **Alternatives:** autosave (nothing lost, but contradicts explicit Save and blurs draft vs
  public); plain Save with no state line (zero work, no unsaved warning); separate Edit and
  Preview screens as Bumble (preview is the truth, more routing).
- **Cost:** S.

#### F2. Publish confirmation

- **Recommendation:** after the preview, one block above Publish: the exact public fields as
  read-only rows; one sentence that published data is visible to everyone and can be copied even
  after deletion; two separate unticked checkboxes ("I am 18 or older", "I understand this profile
  is public"); Publish disabled until both are ticked; ticks reset on edit, save, or a failed
  publish that changed the draft. Progress with Cancel (keeps the draft); failure as a message-bar
  error with Retry; success as persistent "Published". No modal. Add a DOB-derived under-18 block
  when a birth year exists.
- **Pros:** consent sits at the action; two statements avoid bundled consent; matches existing
  markup.
- **Cons:** friction may become reflex; self-attested age is unverifiable; without a modal a user
  can scroll past the warning.
- **Alternatives:** single combined checkbox (less friction, bundled consent); modal alert dialog
  (hard stop, needs focus trap, users click through); DOB gate only (real validation, but DOB is
  optional); typed confirmation word (hostile for accessibility and translation).
- **Cost:** S. Conflict with P3: modal vs inline, see Q6.

#### F3. Search form and results

- **Recommendation:** one compact filter form in profile order and vocabulary; country uses the
  profile select (replace the raw two-letter input); only offered fields; Search is the single
  primary action and Enter submits; query kept across navigation; always show "Results are a
  sample, not a complete list" and, after a run, an applied-filters line; one row per verified
  profile and a "Load another sample" button; one shared status region (StatusPage-shaped) for
  loading, empty ("this sample had no verified matches"), partial, and each typed failure with
  Retry; error text selectable; focus moves to the region, `aria-live="polite"`.
- **Pros:** reuses profile controls; sample wording prevents "nobody matches me"; typed failures
  already exist in the Rust layer.
- **Cons:** persistent notice adds noise; the country list is needed on the search page; sorting and
  compatibility display are undecided.
- **Alternatives:** filter chips with popovers (compact, but five filters do not need them);
  age slider (familiar, custom dual slider is weaker for keyboard and screen readers); live search
  while typing (rejected: each query asks up to three nodes with proof of work and rate limits);
  infinite scroll (rejected: results are a sample, not a stream).
- **Cost:** M.

### 8.6 Settings decisions

#### S1. Settings grouping

- **Recommendation:** one Settings screen with four sections in a left list (top tabs when
  narrow), scope named in each title: **Identities** (list, create, switch, Peer ID, key storage,
  backup instructions, and last a separated "Remove identity" row); **Network** (per identity:
  status line first, bootstrap nodes and name behind "Advanced", "Check and apply"; heading shows
  the Peer ID); **Storage and backup** (app-wide, read-only in v0.0.1; export marked planned);
  **About and updates** (version, update interval, last check, language).
- **Pros:** matches KeePassXC (app vs database) and Thunderbird (per account); hero status line
  follows Windows; Remove at the end like Element and Thunderbird; unfinished controls stay visible
  but disabled.
- **Cons:** moves the node form out of the identity pane and changes the current flow; the Storage
  group is nearly empty and may feel padded; an identity-scoped Network page can be mistaken for
  app-wide.
- **Alternatives:** keep one scrolling page (zero work, remove button in the routine scroll);
  Thunderbird-style tree with identities as nodes (obvious scope, heavy for 1-2 identities, large
  rewrite); two dialogs as KeePassXC (clean scope, two entry points).
- **Cost:** M.

#### S2. Tiered removal

- **Recommendation:** keep the dedicated remove view. Step 1: "Remove identity..." row in a danger
  area, disabled for the active identity unless the backend flow allows it. Step 2: exact Peer ID,
  storage kind and a consequence list; destructive-styled button naming verb and object; Cancel
  focused by default; Enter does not confirm; Esc cancels. Tier A (another copy exists, backed up,
  or key already missing): checkbox. Tier B (last known local copy): additionally type the last six
  characters of the Peer ID (or a fixed word), with a backup link beside it. After success, a result
  line naming the removed Peer ID; no identity is silently selected. No undo toast.
- **Pros:** friction scales with irreversibility; typing a Peer ID piece forces identification;
  reuses the existing view and last-copy check.
- **Cons:** type-to-confirm is awkward for assistive technology and can be pasted; slower for
  developers; relies on "last copy" being accurate (it cannot see backups outside the app, so the
  wording must say "the only copy this app knows about").
- **Alternatives:** single checkbox always (no work, last-copy removal as easy as any); undo toast
  with deferred destruction (best flow, key stays on disk, crash window ambiguous); soft delete or
  "forget but keep key file" as Firefox (recoverable, leaves secrets on disk, needs a recovery UI);
  always typed (uniform, annoying).
- **Cost:** S.

#### S3. Backup warning touchpoints

- **Recommendation:** three short factual touchpoints, no dismissable nag: at creation (already for
  file backend); a persistent line in Identities and Storage and backup stating where the key is and
  which file to copy; the consequence list in the removal view. Export and restore stay marked
  planned and disabled.
- **Pros:** follows Element and KeePassXC; no global banner.
- **Cons:** static text can be ignored; users who never open Settings see nothing between creation
  and removal.
- **Alternatives:** creation and removal only (minimal, no place to re-read); periodic banner (more
  reach, nagging, no supporting evidence); block removal of the last copy until export (strongest,
  export is not implemented so it would block removal entirely).
- **Cost:** S.

#### S4. Node options under Advanced

- **Recommendation:** bootstrap list and network name under a collapsed "Advanced" `<details>` in
  Network; status and Retry line and "Check and apply" always visible; a failed check keeps the
  previous working configuration.
- **Pros:** mirrors Windows' separate advanced page and Thunderbird's server page; native element.
- **Cons:** hides options early testers need; one more click.
- **Alternatives:** always expanded (discoverable, noisy); separate "Advanced" group (clearer, a
  fifth group for two fields).
- **Cost:** S. Conflict with N1: N1 keeps seeds visible; see Q2.

### 8.7 References decision

#### R1. Two-tier reference sources

- **Recommendation:** tier 1 official docs and help pages for flow steps; tier 2 Flathub
  screenshots for settings, search, profile and main-window layout; for the five gap flows rely on
  the text findings and treat screenshots as optional. Re-run the link checks before each design
  review.
- **Pros:** free, official, linkable, HTTP-checked, licence-safe; docs images show real flow steps.
- **Cons:** docs images are old (2020, 2022-23) and unversioned; links may rot; five flows
  uncovered; images not opened.
- **Alternatives:** run the reference apps in Docker or Flatpak and capture own shots (exact UI,
  already rejected for sandbox and cost); read source only (no staleness, no visual check); store
  local copies under `build/` (survives rot, licence and artwork exposure); Flathub
  only (one API, misses flow screens).
- **Cost:** S. A human pass over the roughly 30 "caption only" images takes about 30 minutes.

## 9. Open questions for the maintainer

Merged and deduplicated. Q1 to Q7 are conflicts between reports that need a decision in the IA.

| # | Question | Where it matters |
|---|----------|------------------|
| Q1 | **Where do the account switcher and network settings live?** Header switcher (K4), footer status with "Edit network" (N3), or only inside Settings, Identities and an identity-scoped Network section (S1)? Reports differ; the IA must choose or combine (switcher in header, details in Settings) | K4, N3, S1 |
| Q2 | **What sits under the Network disclosure?** N1 hides only the network name; S4 hides seeds and name behind Advanced. Seeds have no default, so hiding them is questionable | N1, S4 |
| Q3 | **Apply behaviour for network:** explicit Apply with pending state (P5, S1) vs the check-then-Continue flow (N2); and instant-apply or a revert for everything else? | P5, N2 |
| Q4 | **Backup nudge:** dismissible "Not backed up" row (K3) vs no dismissable nag (S3). For system-store identities there is nothing to back up: ship an export in v0.0.1, or accept "not backed up" with a warning and how to dismiss it? | K3, S3 |
| Q5 | **Restore in v0.0.1:** first-screen Restore (K1, K5) vs export and restore "planned and disabled" (S3). Accept only a whole `profiles/<slot>/` folder or also a bare `identity.key`? | K1, K5, S3 |
| Q6 | **Confirmation form:** native `<dialog>` (P3), dedicated remove view (S2), inline no-modal publish block (F2). Is a mix acceptable, and which single dialog button order applies (Cancel left or action left)? | P3, F2, S2 |
| Q7 | **Shell and viewport:** is the 1024x600 floor acceptable, or must the UI work near 400 px like Android? Settings left list collapsing to top tabs (S1) vs the sidebar collapse at 640 px (P1) | P1, S1 |
| Q8 | Is a macOS target planned near term? It affects the sidebar, Cmd+, and button order | P1, P3 |
| Q9 | Follow only the OS light, dark theme, or add a choice in settings? | P2, P5 |
| Q10 | May the active identity be removed directly, or must the user switch first? Is a "forget but keep key file" soft path wanted before v0.0.1 (it changes the "removed means destroyed" semantics)? | S2 |
| Q11 | Type-to-confirm token: Peer ID suffix or a fixed word? Accessibility with screen readers is unresearched | S2 |
| Q12 | Is a user-visible local label per identity wanted, given the local-only Peer ID rule? | K4 |
| Q13 | Optional passphrase for file storage: in scope for v0.0.1 or later? | K2 |
| Q14 | What does "Connected" mean (any seed, N seeds, or network name verified)? Typical seed count, check and per-row timeouts (measure, do not copy), and how many users use a private network (decides if the name stays collapsed)? Auto-connect on launch needed, or is skipping the form when an active network exists enough? | N1-N3 |
| Q15 | Should failure messages show an expandable technical detail line? Needs i18n review | N2, F3 |
| Q16 | Can the protocol unpublish or withdraw a profile, or only publish a newer one? A "stop being found" control depends on it | F2 |
| Q17 | Typical proof-of-work time on a desktop (pending wording, progress bar) and the under-18 block when a partial date's age range straddles 18 | F2 |
| Q18 | Preview as in-page section (F1) or a separate screen? Needs a quick check with 3-5 people. Every new string needs all locales: check the catalog process | F1 |
| Q19 | May a follow-up read the pages that failed (section 10), and run the reference apps for the five screenshot gaps? | all |

## 10. Limits

- **Failed to load:** GNOME HIG preferences and progress pages (404); Microsoft spacing page
  (404); m3.material.io pages (JS only); KDE detail pages (only the landing page was read); Briar
  manual (TLS failure from the research host in one run, read in another); Signal support backup
  article (403); Signal support and Mozilla support pages (JS shell, no static images); KeePassXC
  user guide body (only TOC and interface sections); SimpleX web host (unreachable in one run, clone
  docs used); Molly and GNOME Settings Flathub ids (404); guessed Jami backup and link-device doc
  URLs (404).
- **Partial reads:** Apple alerts page (read via the HIG JSON feed, partial). Apple sidebars and
  settings were read the same way.
- **Search snippets only:** Jami bootstrap and FAQ, Element homeserver issues and config doc,
  libadwaita preferences and Toast classes, StatusPage text selection thread, Windows Settings
  structure, Element sessions and deactivate wording, Signal unlinking, Mozilla account and
  profile-manager pages, Mastodon fedi.tips, NN/g, Bumble app listing. Re-verify before quoting.
- **Third-party or lower trust:** qBittorrent status behaviour (forum and a blog; no official
  document found), the lobehub GNOME summary. No official 18+ verification page was found for
  Tinder or Bumble, and no official Bumble or Tinder age and distance filter page.
- **Source code, not running apps:** Session, Jami, SimpleX, Briar, Element, Signal Desktop, Fractal,
  KeePassXC and Secrets were studied through shallow clones (string keys, layout files, settings
  code), never built or run. Briar has no desktop UI, so its layout is read from the Android string
  catalog. The clones are untrusted data and are not committed.
- **Screenshots:** image content was not opened; about 30 rows rest on captions, alt text or doc
  text. SimpleX images date from 2022-2023 and Syncthing's from 2020.
- **Not run or tested:** no prototype, usability test or measurement. Webview behaviour claims
  (engine support, `<dialog>` focus behaviour, `AccentColor`, `forced-colors`) are untested on
  WebView2 and WebKitGTK. Assumptions that need validation: typical seed count (1-5), that users
  read short warnings, proof-of-work duration, and the button-order choice.
- **Repository context read:** `clients/desktop/ui/index.html`, `clients/desktop/ui/app.js` and
  [v0.0.1-client-screens.md](v0.0.1-client-screens.md); the descriptions of current behaviour above
  come from those files and may drift.
