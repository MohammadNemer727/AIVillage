# 8B: semantic graph consolidation

**Result date:** 2026-10-23
**Protocol:** [`semantic-graph-8b-preregistration.json`](semantic-graph-8b-preregistration.json),
frozen at commit `e6ee9a8` before any of its eight seeds ran.
**Machine report:** [`semantic-graph-8b-result.json`](semantic-graph-8b-result.json), sha256
`6e1ba8ae5d02c34dfb8be04629accd7dcbbb6a6e937d7c8e3f031f383f520ab1`. Raw worlds and traces stay out
of git.

## Decision

**Accepted under the frozen rules, narrowly and for this fixture only.** Consolidating episodes
into a bounded semantic graph with contradiction handling beat raw policy-only episodic retrieval
when the consequences of the two actions swapped mid-run, and matched it when they did not.

| Check | Result |
| --- | --- |
| Validity: all 48 arms valid, all 48 traces replay exactly | passed |
| Primary (reversal, G against B): post-reversal returns +16 and losses −16 or better in aggregate, at least 6 of 8 pairs better, at most 1 worse, graph actually fired | **passed at the margin** |
| Non-inferiority (stationary, G against B), and still better than no memory | passed (identical to B) |

One arm (`stationary-seed-743-graph`) shut down without writing a result and was rerun once under
the frozen infrastructure rule. Its first attempt is kept as `.failed` in the run directory and was
not looked at.

## Primary: after the mapping reversed (8 seeds, 40 scored choices each after the swap)

| Arm | Post-reversal returns | Post-reversal losses |
| --- | ---: | ---: |
| A no memory | 153 | 167 |
| B raw policy retrieval | 95 | 225 |
| G semantic graph | 227 | 93 |

Per-seed post-reversal returns (A / B / G):

| Seed | 709 | 719 | 727 | 733 | 739 | 743 | 751 | 757 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A | 22 | 23 | 21 | 21 | 17 | 17 | 12 | 20 |
| B | 0 | 33 | 31 | 0 | 28 | 0 | 3 | 0 |
| G | 38 | 37 | **4** | 37 | 37 | 0 | 37 | 37 |

G was better than B in 6 pairs, worse in 1 (seed 727: 4 against 31) and tied in 1 (seed 743, where
neither memory recovered). It sits exactly on the pass line for positive pairs, so this is a real
but not a comfortable pass.

The most informative number is that raw retrieval did **worse than no memory** after the swap (95
against 153). Its remembered consequences kept recommending the action that used to relieve the
body, and in four seeds it never changed its mind. The graph halves the support of a node whenever
the same action is seen with a different outcome in a similar felt state, so it follows the new
world. In the graph arms 74 contradictions were applied and 786 actions were changed by retrieval.

## Non-inferiority: the world does not change (8 seeds, 40 choices each)

| Arm | Returns | Losses | Policy flips |
| --- | ---: | ---: | ---: |
| A no memory | 140 | 180 | 82 |
| B raw policy retrieval | 320 | 0 | 0 |
| G semantic graph | 320 | 0 | 0 |

The graph made the same 40 correct choices as raw retrieval on every seed, so consolidation did not
cost anything where raw retrieval was already perfect.

## Bounds and churn

The graph never exceeded 10 of its 16 nodes and evicted none; 64 nodes were created across the 16
graph arms. Prediction is untouched by construction: the graph feeds only the policy, and a unit
test (`enabling_the_graph_cannot_change_any_prediction`) shows every prediction is identical with it
on or off.

## What this does and does not show

- It shows that, in this controlled two-action fixture, felt-state-matched consolidation with
  contradiction handling lets a body follow a reversed world that raw retrieval cannot.
- It does not show general semantic reasoning, planning, or benefit outside a world where one
  action's consequence flips. The fixture's only "world change" is that single swap.
- The design was changed after development pilots (seeds 9101–9103, consumed). The first version
  matched contexts on a recurrent latent that drifts too far between choices and failed outright;
  the version tested here matches on the felt drive state. Thresholds were set after those pilots
  and before any preregistered seed ran. The final mechanism was not tuned on the eight seeds above.
- Graph work does not feed the evolution experiments yet: the plan gates that on this decision,
  and Experiment 002 gave no evolutionary question for it to answer.
