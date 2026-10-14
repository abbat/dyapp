---
name: analyse
description: Think through a problem, design, decision, audit, review, or failure investigation using evidence-driven Five Whys and explicit trade-offs. Use for requests to analyze, investigate, design, audit, review, or compare options; not for straightforward read/edit/run tasks without an analytical question.
---

# Analyse

Use the final Five Whys algorithm below to turn a concrete question into an
evidence-backed explanation or decision and a testable next step. Five is a
heuristic, not a required depth. Scale effort to uncertainty, impact, and the
user's scope. Design and choice use the adaptation below; they do not require
inventing a failure or a single root cause.

## Final algorithm

1. **Frame.** Define the observed deviation from an expected state: what,
   where, when, magnitude, and impact. Record scope and a measurable success
   criterion. Separate observations, stakeholder requirements, and assumptions.
   Gather relevant artifacts and practitioner knowledge; do not invent either.
   Authorized containment and evidence collection can proceed before diagnosis.
2. **Ask and branch.** For the current effect, ask what mechanism and conditions
   produced it. Each candidate cause becomes the subject of the next why.
   Write one concrete claim per node; connect cause to effect. Retain material
   competing explanations and interacting causes. Distinguish AND conditions
   from alternative OR paths. Prioritize by evidence, impact, and the value of
   the next check; record deferred branches and residual risk.
3. **Check each link.** Attach supporting evidence and provenance, contradictory
   evidence, and a status: supported, hypothesis, rejected, or unknown. Check
   temporal order, mechanism, and whether the claim adds information rather
   than restating the effect. Read the chain forward with "therefore" to find
   gaps; logical fluency and team consensus are not empirical proof. Seek a
   discriminating observation or test against a credible rival explanation.
   With insufficient evidence, collect it or leave the branch explicitly open;
   never promote an imagined continuation into a finding.
4. **Test intervention relevance.** Ask what would change if the candidate
   cause or causal link were controlled, with context held explicit. In
   multi-cause systems, controlling one contributor can reduce risk without
   eliminating the outcome; do not require `not cause => not effect` at every
   link. Separate why a defect arose from why detection or containment failed.
   A missing record does not, by itself, prove the work was not performed.
5. **Decide whether to continue.** A branch is ready for an intervention when
   its evidence and mechanism explain the scoped effect, a specific cause or
   link can be influenced, the proposed intervention has a testable expected
   benefit, and material competing paths are addressed or disclosed. This is a
   useful intervention boundary, not proof of an ultimate or unique cause.
   Continue if an unresolved link could change the decision. If evidence,
   access, expertise, or scope blocks progress, report an unresolved result
   and the smallest next check or escalation. Repetition, a fifth answer,
   consensus, or reaching "human error" is not a successful stopping rule.
6. **Compare and plan.** Recommend the smallest adequate intervention or
   experiment supported by the evidence. Consider direct technical fixes,
   prevention, detection, and process changes where relevant. Training or a
   new procedure needs a demonstrated mechanism; it is not an automatic cure.
   Always give the recommendation's **pros and cons**. When viable alternatives
   exist, give each its own **pros and cons** and explain the choice. Identify
   the action, proposed owner, acceptance measure, and review time or trigger;
   label unassigned owners and proposed dates instead of inventing commitments.
   Analysis alone does not authorize implementation or sending messages.
7. **Verify and learn.** Within authorized scope, implement or test; otherwise
   provide the reviewable plan. Compare results with the baseline and success
   criterion over relevant exposure or a stated observation window. Check side
   effects and remaining causal paths. A passing test supports only its scope;
   absence of recurrence does not guarantee permanent prevention. Reopen the
   failed branch when results contradict the prediction. Preserve the compact
   evidence record and update relevant standards or documentation when warranted.

## Formal state and transitions

Maintain only the detail needed for the decision:

- Input: `P` (problem or decision), `S` (scope), `E` (evidence), `K` (criteria).
- State: causal graph `G`, branch frontier `Q`, unresolved set `U`, options `A`.
- Edge: `(cause, effect, mechanism, context, evidence_refs, status)`.
- Transition: `frame -> ask -> check -> branch/refine -> assess boundary`.
- A supported actionable boundary adds an option to `A`; a rejected edge is
  excluded; a consequential unknown requests evidence or enters `U` with a
  next check. Independent supported work may continue.
- Termination: a justified recommendation with validation plan, or an explicit
  unresolved result with the evidence needed to decide. Neither implies that
  every cause is known or that a proposed action has already succeeded.
- Feedback: `compare -> plan/test -> measure -> accept within scope OR reopen`.

## Application modes

- **Debugging / postmortem:** start from observed behavior and a timeline;
  distinguish the trigger, contributing conditions, and failed barriers.
- **Audit / review:** first establish each finding with a concrete artifact,
  trigger, and consequence. Recommend a direct correction when sufficient.
  Investigate organizational causes only when evidence and scope justify it;
  a useful review does not require guessing why the author made a mistake.
- **Design:** start from the desired outcome and constraints. Ask why each
  requirement matters and what would prevent success; distinguish stated
  needs from hypotheses. Compare mechanisms and validate risky assumptions
  through prototypes or measurements. A goal-to-requirement link is a
  justification, not an observed causal fact.
- **Decision / general analysis:** identify criteria and assumptions that could
  change the choice, investigate those assumptions, and compare viable options
  against the same criteria. Include the status quo when genuinely viable.

For many interacting causes, use a causal graph or fault tree; for broad cause
search, use Ishikawa before deepening selected branches. Use experiments when
observations cannot distinguish explanations. These complement Five Whys;
none turns unsupported claims into facts.

## Concise output

Lead with the conclusion and its confidence or limitation. Provide a compact
causal/evidence summary (or requirement/assumption map), the proposed action and
verification, and unresolved material issues. **Every summary must include
pros and cons of the recommendation and, when available, alternatives with
pros and cons for each.** Do not manufacture alternatives just to fill a list.
For short tasks, a few sentences or a small comparison table are sufficient.

Use tools for evidence, not to generate plausible causes. Reuse already-read
artifacts and batch independent checks. Ask the user only for consequential
missing information. Report decision-relevant reasoning and evidence, not an
exhaustive private deliberation transcript.

The algorithm is an AI-agent adaptation, not a verbatim Toyota standard or an
experimentally validated guarantee.
