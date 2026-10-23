# Experiment 003: episodic memory integration

**Result date:** 2026-10-23
**Protocol:** [`experiment-003-preregistration.json`](experiment-003-preregistration.json) with
[`amendment-1`](experiment-003-amendment-1.json) (frozen before any outcome was inspected).
**Machine report:** [`experiment-003-result.json`](experiment-003-result.json), sha256
`487b0984247e081a3e3da93087bff0be766b66605fdff2b6c02699b884886ba7`, produced from the snapshot of
revision `5df34b9`. Raw worlds and traces stay out of git (see `.gitignore`).

## Decisions

| Question | Decision |
| --- | --- |
| Policy-only episodic retrieval (B) | **Accepted** |
| Prediction-integrated episodic retrieval (C, D) | **Rejected**; prediction retrieval stays disabled |

Validity held: every fixture was valid, all 32 live traces replayed exactly, and the frozen replay
confirmed the policy switch cannot change prediction (A equals B and C equals D exactly). No arm
needed an infrastructure rerun. Attempt 1 (seeds 601–647) was retired as incomplete before any
result was opened, as recorded in the amendment.

## Policy (live paired, 8 untouched seeds, 40 scored choices each)

| Arm | Resource losses | Resource returns | Pairs better than A |
| --- | ---: | ---: | --- |
| A off | 152 | 168 | — |
| B policy | 4 | 316 | 8 of 8, none worse |
| C prediction (diagnostic) | 147 | 173 | 4 better, 3 worse |
| D both | 0 | 320 | 8 of 8, none worse |

B met every frozen criterion: losses fell by 148 and returns rose by 148 (threshold 16), return
deltas were +22, +16, +21, +19, +20, +12, +19, +19, and mediation was complete (40 positive outcome
writes, 747 retrievals with body evidence, 478 actions changed by retrieval, 471 of those followed
by a nonnegative outcome and none by a negative one). C changed no actions, as expected. No arm
died; the lowest felt health was 191/255 in A and 242 in B.

## Prediction (frozen replay on the same recorded transitions)

| Modality | A and B (off) | C and D (retrieval on) | Paired transitions (on vs off) |
| --- | ---: | ---: | --- |
| Vision MAE | 21.910 | 21.910 | 0 better, 718 tied, 0 worse |
| Body MAE | 3.458 | 4.914 | 48 better, 591 tied, 79 worse |
| Held item | 0 | 0 | tied (no variation) |

The preregistered tolerance was zero, so prediction retrieval fails its criterion (C against A and
D against B). It made body prediction worse for the second untouched set of seeds, in line with the
expectation recorded before the run. Online one-step error on each arm's own life is diagnostic
only because the arms' trajectories differ.

## What this does and does not show

- Retrieval of private body consequences can make the founder pick the relieving action in this
  fixture, reliably across seeds. That is a **controlled repeated-choice fixture**: two eligible
  actions whose bodily consequence a test hook applies. It shows the retrieval mechanism works
  causally; it is not evidence of general foraging, survival, or intelligence in the open game.
- Prediction retrieval is not validated and remains off. The body-prediction regression is now seen
  on 16 untouched seeds across two protocols.
- This opens the planned next stages on the policy-only configuration (B). It does not open
  prediction-integrated retrieval.
