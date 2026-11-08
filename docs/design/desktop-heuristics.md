# Desktop heuristic review

Every change to the [desktop prototype and shipped UI](desktop-screens.md) is reviewed against
[Nielsen's heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/) and the
[UI review criteria](review-criteria.md) (ADR 0046). This file holds the method, open findings and
accepted exceptions only; closed reviews stay in git history.

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

Since 2026-11-07 each review also applies [UI review criteria](review-criteria.md) to the whole
screen: settings S1-S8, layout L1-L2, identifiers I1-I4, dialogs and text. These heuristics miss
structure and spacing. A failed criterion is severity 3 and blocks the screen.

## Open findings

None.

## Accepted exceptions

None.

## When to review

Before any change to the prototype or to a client screen, re-check the changed states and their
neighbours against H1–H10 in both languages at 440 and 1280 px. Record a finding here only while
it is open, with the state, heuristic, problem and planned fix; delete the row when the fix lands.
A change ships with no open finding of any severity (1-4). The only way past a finding is an
exception the maintainer accepted, recorded here with the reason and the date; an agent never
accepts one itself. This is the heuristics part of practice [D3](../development/practices.md); the
human approval part is not adopted yet.
