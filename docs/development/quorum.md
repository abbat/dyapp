# Running a quorum

How a human runs a quorum: several AI models review one aspect of the project independently,
challenge each other's findings, and one model turns the result into a work plan. The procedure
the models follow is [agents/quorum.md](../agents/quorum.md); this page only covers what you type.

In the commands below, `<model>` is the participant's name with its version, e.g.
`claude-opus-5.5` or `codex-gpt-5.2`; use the same name everywhere for one participant. `<Q>` is
the quorum epic's ID.

## 0. Prepare

- Pick the question, at least two participants and the synthesizer (the strongest model you
  have).
- Make sure every participant's harness has `bd`, lean-ctx and the `analyse` skill, the same
  version for all. A model without them stops and tells you.

## 1. Charter

In any session, in the repository root:

```
Read docs/agents/quorum.md. Open a quorum on: <question>. Draft the charter with me.
```

Agree on scope, participants, synthesizer and budget. Keep your own hypotheses out of the
charter: they anchor every participant. The model creates the quorum epic and gives you `<Q>`.

## 2. Phase 1: blind research

Start one session per participant, in parallel if you like:

```bash
BEADS_ACTOR=<model> claude    # or: BEADS_ACTOR=<model> codex
```

```
Read docs/agents/quorum.md. You are participant <model> in quorum <Q>, phase 1.
```

Models only read by default; they ask you before running tests, builds or anything else.

## 3. Phase 2: cross-review

Check the phase log:

```bash
bd comments <Q>
```

When every participant has posted `phase 1 complete`, give each one (same or new session; the
state is in beads):

```
Read docs/agents/quorum.md. You are participant <model> in quorum <Q>, phase 2.
```

## 4. Phase 3: synthesis

When every participant has posted `phase 2 complete`, start a **new** session of the
synthesizer:

```bash
BEADS_ACTOR=<model> claude
```

```
Read docs/agents/quorum.md. You are the synthesizer of quorum <Q>, phase 3.
```

## 5. Decide

The synthesizer shows the plan and the `escalated` questions. Answer the escalations and approve
the plan; it then closes the quorum. `bd dolt push` is your separate decision; hold it while
`security` findings are unfixed.

## If something goes wrong

- A participant crashed or ran out of context: start a new session with the same prompt; it
  resumes from beads.
- A participant is useless or unavailable: ask any session to close its participant epic with
  `withdrawn: <reason>`. At least two participants must remain.
- Someone wants to join late: possible only before anyone starts phase 2; otherwise open a new
  quorum.
