# UI review criteria

These criteria come alongside the heuristic review in
[desktop-heuristics.md](desktop-heuristics.md). Nielsen's heuristics judge
interaction one step at a time. They miss problems of structure, where an
option lives and which control it uses, and they miss spacing. Each criterion
below is a yes-or-no check with a source. Apply it to the whole screen at 440
and 1280 px, in English and Russian. A failure has severity 3 and blocks the
screen. If a failure is deliberate, record it in the screen page with its
reason. The decision is in [ADR 0046](../decisions/0046-ui-review-criteria.md).

Several criteria are house rules: the sources support them, but no standard
gives a measurable rule for them. The [Gaps](#gaps) section lists which ones.

## Settings (S1-S8)

The criteria apply to every settings screen and to each pane of a settings
window.

| ID | Criterion | Source |
|---|---|---|
| S1 | Scope: every item has a stored value and a default. A collection with add and remove (identities, devices, blocked peers) appears only as a drill-down row with a count. | Apple HIG Settings: general, infrequently changed settings |
| S2 | Bounded: no list that grows with use sits on a settings screen. Such a list gets its own screen with paging and search. | Android, Organize your settings: split large screens into subscreens |
| S3 | Cohesion: one short group name covers all its items. A name that needs "and" means the group should be split. | Rosenfeld and Morville, *Information Architecture*: exclusive organization schemes; Android: group related preferences |
| S4 | Locality: an option that affects one task lives in that task, not in Settings. | Apple HIG Settings: task-specific options |
| S5 | Control fit: an immediate binary setting is a switch, and a checkbox appears only on a form with a submit button. Exceptions: a checkbox for a hierarchy of settings, radio buttons for 2-5 exclusive choices, and a menu or pop-up for more than about five. A quantity is never an enumeration: give a number with its unit or a slider, the current usage and a clear action. | Microsoft toggle switches (a toggle takes effect at once); NN/g toggle-switch guidelines; Apple HIG toggles |
| S6 | Length: the root fits one viewport at 1280 px and two at 440 px, with about 10 controls per category at most; more controls go behind progressive disclosure. | Apple HIG Settings: minimize the number of settings; NN/g progressive disclosure |
| S7 | Destructive actions never share a screen with appearance preferences. | ISO 9241-110:2020, controllability and use error robustness |
| S8 | Theme and language default to the system. | Apple HIG Settings: respect systemwide settings |

## Layout (L1-L2)

These apply to every screen.

| ID | Criterion | Source |
|---|---|---|
| L1 | Proximity: the gap between a block and a neighbour from another group is at least the largest gap inside either group. In tokens, gaps inside a group are at most `--space-2` (8 px) and gaps between groups at least `--space-3` (12 px). | NN/g Gestalt proximity; KDE HIG metrics: related items 2 x smallSpacing, a label and its item 1 x, unrelated controls at least 3 x |
| L2 | No touching: distinct sibling blocks (controls, text, notices) are never closer than `--space-2` (8 px). A label or legend may sit closer to its field. Gaps and margins use the `--space-*` tokens on the 4 px grid. | Fluent 2 layout: 4 px grid; Apple HIG Layout |

Static presentation styles live in CSS. Repeated colours and font properties
with the same semantic purpose use shared CSS custom properties; equal values
with different purposes remain separate. One-off colour and font values need
not be tokens. This maintainability convention does not add visual criteria
to L2; its spacing-token requirement still applies.

KDE also gives minimum sizes: a button 72 x 32 px, a line edit at least
80 x 32, and a checkbox or radio button with its label at least 80 x 24.
WCAG 2.2 SC 2.5.8 requires targets of at least 24 x 24 CSS px.

## Identifiers (I1-I4)

An identifier is any string the action targets, such as an ID, key, hash,
link, file name or path.

| ID | Criterion | Source |
|---|---|---|
| I1 | Separation: an identifier is its own block, with at least `--space-3` (12 px) above and below. By L1, it never sits closer to the prose below than to its title above. | NN/g Gestalt proximity |
| I2 | Distinct presentation: monospace plus a container (a subtle background or border) or a visible label such as "Pack ID", so it can be told apart from prose without being read. The markup exposes the relation too, through a label or dt/dd. Every identifier uses the one shared identifier class. | WCAG 2.2 SC 1.3.1 Info and Relationships; ISO 9241-112:2017, discriminability |
| I3 | Chunking: a long identifier is shown in fixed groups (for example 4 or 8 characters), with breaks between the groups and not at arbitrary points. Copying it yields the raw string. | NN/g chunking |
| I4 | Confirmation order: a destructive confirmation names the object first (its name, then its identifier), then the consequence, then the actions. | NN/g confirmation dialogs |

## Dialogs and text

| ID | Criterion | Source |
|---|---|---|
| D1 | A dialog title wraps to at most two lines in the frozen matrix: English/Russian at 440/1280 px and normal text size. Stress variants check clipping and accessibility without imposing this line limit. | Apple HIG Alerts |
| D2 | The button that dismisses without acting is labelled "Cancel" ("Отмена"). | Apple HIG Alerts |
| D3 | A dialog has exactly one default button, except a selection dialog may have zero or one. Outside dialogs a view may have zero or one. Initial focus on Cancel does not make it the default. | Project rule confirmed by the maintainer; GOV.UK button: avoid multiple default buttons on a page. Apple HIG Alerts permits no default for some alerts; this project rule is stricter. |
| T1 | Text expansion: a control holds text 30% longer than the English for strings of 70 or more characters, and proportionally more for shorter ones. Check with the Android en-XA pseudolocale and with Russian at 440 px. | Adobe Spectrum, International design |
| T2 | Text and controls meet WCAG 2.2 contrast (SC 1.4.3), and focus is visible (SC 2.4.13). | WCAG 2.2 |

## Fixing a screen

The [zero-baseline rule for changed files](../development/practices.md#rules-shared-by-all-practices)
also applies to UI work: fix every baseline violation in the affected screens
across checked languages, widths, themes and text sizes, and remove resolved
accessibility/reflow and layout entries in the same change. Do not invent new
UI rules; ask the maintainer when a criterion needs clarification. A new UI/UX rule
requires an explicit maintainer decision with its rework cost (screens × platforms × tests);
until then, follow the existing contract. Before removing UI rationale, preserve the
established decision in a regression test or screen-contract line.

## What is automated

- **S5 and S2 as a ratchet.** `tests/scripts/check-repository.py` counts the
  checkboxes and the list rows (`tr`, `li`, `.list-item`) inside each
  prototype's `<section id="settings">`. The counts must equal the budgets under
  `[settings]` in `tests/scripts/baselines.toml`. A count above its budget
  fails. When a count falls, the check also fails until the budget is lowered
  to match.
- **L2 spacing tokens and CSS ownership.** The web lint step checks static
  styles live in CSS and gaps/margins use spacing tokens. Repeated colour/font
  literals require shared tokens or a recorded difference in semantic purpose;
  one-off colour/font literals remain allowed. See
  [web lint](../development/linting.md#web-biomejsonc) for the check and its
  shrink-only spacing budget. These static checks do not replace gap measurement.
- **L2 and I1 by measurement.** When `tests/scripts/ui-test.sh prototype` (and
  `prototype-android`) takes screenshots, `tests/scripts/prototype-shots.py`
  also measures, for every state, language and width, the gap between
  neighbouring visible blocks. It writes the result to `layout.json` next to
  the screenshots. [check-layout.py](../../tests/scripts/check-layout.py) then
  applies L2 (a framed block, meaning a control or a block with a border or
  background, closer than 8 px to its neighbour) and I1 (`.full-id` or a block
  `code` closer than 12 px). A finding is named by its scope and both blocks,
  whatever the screen. The findings known today are listed under `[layout]` in
  `baselines.toml`. A new finding fails, and a fixed one must be removed from
  the list. Some listed findings are false positives of the measure, such as
  list rows that touch on purpose or a dot beside its text: mark them with a
  comment.
- **D1-D3 on desktop prototypes.** D1 measures titles in the frozen English/Russian
  440/1280 px normal-text matrix. Every capture checks the translated Cancel key
  and default-button count. `.primary` or `data-default` marks a default; `.focus`
  is only an initial-focus hint. Selection dialogs declare `data-dialog-kind="choice"`
  and may have no default. Three-line titles, incorrect Cancel keys and missing/duplicate
  defaults are exercised as failing counterexamples.
- **T1 reflow and text expansion.** Every state runs in light/dark themes, normal/200%
  text and 320/440/1280 px, in English/Russian. Desktop `en-XA` at 440 px applies the
  Spectrum expansion bands (total lengths of 300/200/180/160/140/130%, with the frozen
  70+ boundary at 130%). Text clipping without a scroll container and horizontal page
  scrolling are measured; new findings fail and fixed ones must leave the shrink-only
  `desktop-ui-baseline.json` budget.
- **T2 and target size.** Pinned axe-core 4.14.0 explicitly enables all WCAG 2.0–2.2
  A/AA tagged rules on every desktop prototype capture and native smoke screenshot
  checkpoint, including disabled-by-default SC 2.5.8. Contrast and target-size findings
  requiring review are recorded too. Low-contrast text and adjacent undersized buttons
  calibrate the audit. Prototype accessibility uses the same shrink-only UI baseline;
  native smoke findings fail directly.
- **Keyboard focus.** Every prototype state in the frozen matrix receives real GDK
  Tab/Shift+Tab events; native radio groups use Tab to enter/leave and arrow keys within.
  Focused/unfocused rendered pixels check SC 2.4.13's 3:1 contrast and two-CSS-pixel
  perimeter area; SC 2.4.11 rejects entirely obscured focus. Escape restores the opener,
  except unsolicited revocation focuses the next screen heading. Traps, missing focus
  indicators and ignored Escape are tested counterexamples. These failures cannot be
  budgeted. See [UI testing](../testing/ui-testing.md#desktop-prototype-screenshots).
- **By hand.** S1, S3, S4, S6, S7, S8, L1 and I2-I4 still need reviewer judgement.
  Native mobile accessibility audits remain deferred.

## Gaps

- No standard gives a measurable rule for settings structure. S1-S3 stay house
  rules with qualitative sources.
- No standard gives a chunk size for identifiers, so I3 stays a house rule.
- No source found specifies which control gets the initial focus in a
  dialog, or the order of the buttons in a Material 3 dialog (that page
  renders only with JavaScript). A live Material settings page has not been
  found.
- Rubric papers worth reading for later reviews: "Towards Generating UI Design
  Feedback with LLMs" (UIST '23 adjunct) and UISGPT.

## Sources

- Apple HIG: [Settings](https://developer.apple.com/design/human-interface-guidelines/settings),
  [Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles),
  [Alerts](https://developer.apple.com/design/human-interface-guidelines/alerts),
  [Layout](https://developer.apple.com/design/human-interface-guidelines/layout)
- Android: [Organize your settings](https://developer.android.com/develop/ui/views/components/settings/organize-your-settings)
- Microsoft: [Toggle switches](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/toggles),
  [Fluent 2 layout](https://fluent2.microsoft.design/layout)
- KDE HIG: [metrics](https://invent.kde.org/teams/vdg/kde-hig/-/blob/master/HIG/source/layout/metrics.rst)
- NN/g: [toggle switch](https://www.nngroup.com/articles/toggle-switch-guidelines/),
  [progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/),
  [Gestalt proximity](https://www.nngroup.com/articles/gestalt-proximity/),
  [chunking](https://www.nngroup.com/articles/chunking/),
  [confirmation dialogs](https://www.nngroup.com/articles/confirmation-dialog/)
- ISO: [9241-110:2020](https://www.iso.org/standard/75258.html),
  [9241-112:2017](https://www.iso.org/standard/64840.html)
- W3C: [WCAG 2.2](https://www.w3.org/TR/WCAG22/),
  [SC 1.3.1](https://www.w3.org/WAI/WCAG22/Understanding/info-and-relationships.html)
- GOV.UK: [Button](https://design-system.service.gov.uk/components/button/)
- Adobe Spectrum: [International design](https://spectrum.adobe.com/page/international-design/)
- axe-core: [rule descriptions](https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md)
- Rosenfeld, Morville, Arango, *Information Architecture*, 4th ed., O'Reilly, 2015
