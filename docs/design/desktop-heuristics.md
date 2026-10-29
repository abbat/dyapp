# Desktop heuristic review

A review of every state of the desktop prototype (`clients/desktop/prototype/`, see
[desktop screens](desktop-screens.md)) against Jakob Nielsen's
[10 usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/). It was
done on 2026-10-29, before the shell was built, and every finding of severity 2 or more is
fixed in the prototype. The rule that keeps it current is under [When to review](#when-to-review).

## Method

Each state was opened in English and Russian at 440 and 1280 px (`make ui-prototype`) and
checked against the heuristics:

| Code | Heuristic |
| --- | --- |
| H1 | Visibility of system status |
| H2 | Match between the system and the real world |
| H3 | User control and freedom |
| H4 | Consistency and standards |
| H5 | Error prevention |
| H6 | Recognition rather than recall |
| H7 | Flexibility and efficiency of use |
| H8 | Aesthetic and minimalist design |
| H9 | Help users recognize, diagnose and recover from errors |
| H10 | Help and documentation |

Severity follows Nielsen's scale: 0 not a problem, 1 cosmetic, 2 minor, 3 major, 4 catastrophe.
A finding names the state, the heuristic, what a user meets and the fix. H7 (shortcuts) and H10
(help) were checked but are out of scope for the prototype: keyboard shortcuts and a help page
are not designed yet.

## Findings

### Severity 3

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `profile-hidden` | H1 | The status still said "Published, version 3. Visible in search." | The status says the profile is hidden and the version stays saved as the draft |
| `profile-view-match` | H3, H5 | One press on Liked took the like back and ended the match for both | Liked opens a confirmation, `unlike-dialog` |
| `settings` | H3 | Block could not be undone: no list of blocked people | A Blocked people card with Unblock (`blocked`, `unblock-1`) |

### Severity 2

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `search-error` | H1, H9 | "Unavailable while offline" while the sidebar said Connected; no way on | The sidebar and top bar show Offline in this state; Open Network next to Retry |
| `network-error` | H1, H9 | "Previous network stays active" next to Offline; no advice; Details open against the spec; Copy indented | The sidebar keeps the previous network's status; the text says nothing changed and to check the addresses with the operator; Details collapsed; Copy aligned |
| Wizard, recovery, removal | H4 | Full Peer IDs in the libp2p `12D3KooW…` form, not the 64 hex characters of ADR 0033; the tail wrapped alone at 440 px | 64 lowercase hex characters on their own line |
| Every identity list | H4 | Short IDs with capitals (`@Fk9Qx2`) | Lowercase hex tails, avatar letters to match |
| `remove-dialog` | H5, H6 | Only the full ID named the identity; no word that its chats go too | Avatar, short ID and kind first; a line that chats, matches and likes on this computer go with it |
| `chat-request` | H6 | "Reply … or block" while Block hid in the ⋯ menu | A Block button in the request notice |
| `chat` | H5 | Delete on an own message acted at once | Delete opens `delete-dialog` |
| `profile-view-match` | H5 | Block had the size and row of Liked, easy to hit by mistake at 440 px | Block is a quiet button under the profile fields |

### Severity 1

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `network` | H1 | "One node answered" while both rows said Answered | "Both nodes answered" |
| `search-loading` | H3 | No way to stop a slow search | Cancel |
| `search-empty` | H2 | "Show another sample" under "Nobody matches yet" | Removed; the advice points at the filters |
| `chat-first` | H2, H8 | "One message is delivered now" next to the Sent mark; text repeated under the composer | The notice states the rule only |
| Chat states | H5 | Send enabled with an empty box | Send is disabled until there is text |
| Chat states | H8 | The composer floated mid-window in short chats | The thread grows and the composer stays at the bottom |
| Shell at 640 px and below | H1 | Connection only as a coloured dot | The top bar shows the status word too |
| `profile-dark` at 440 px | H8 | Selects too narrow, text cut | Grid columns at least 200 px or full width |
| `chats` | H1 | Nav badge 3 against row counts 2, 1 and 2 | The badge counts unread messages and requests (2 + 1) and its label says so |
| `recovery` | H5 | Remove first in the row, next to "Use another identity" | Kept: it is apart from Retry and the dialog needs a ticked box |

States with no finding: `wizard-identity`, `picker`, `publish-dialog`, `publish-progress`,
`network-checking`, `search`, `profile-view`, `chat-menu`, `chat-clear-dialog`, `chat-cleared`,
`switcher`, `menu`, `remove-offline`.

## When to review

Before any change to the prototype or to a client screen, re-check the changed states and their
neighbours against H1–H10 in both languages at 440 and 1280 px, and record each finding of
severity 2 or more here with its fix, or as open with the reason. A change ships with no open
finding of severity 3 or 4. This is the heuristics part of practice
[D3](../development/practices.md); the human approval part is not adopted yet.
