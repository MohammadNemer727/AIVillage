# Private bounded episodic memory

Status: implemented and freshly evaluated; capability gate failed, 2026-10-22.

## Boundary and architecture

Each individual owns a checkpointed store of at most 64 lifetime events. A newborn always starts
with schema 1, encoder version 1, no records, event identity zero, and no forgotten records.
Reproduction carries the experiment configuration but never a parent's records, evidence,
retention, counters, or fingerprints.

An event contains the 16-value recurrent key and the stable 24-value compact sensory source both
before and after the event, the primitive action, predictor uncertainty and surprise, signed
aggregate drive relief plus health/fullness/pain change, body acceleration/vertical
motion/retinal motion, lifetime tick, retention, evidence, contradictions, and a fingerprint.
Retaining source features makes encoder
drift explicit: latent comparisons require the recorded encoder version, while the source
evidence remains interpretable under its fixed body contract. Incompatible encoder versions are
rejected, not silently reinterpreted.

There are no names, labels, object categories, coordinates, item identities, language, task
rewards, other individuals' state, spatial graph, semantic graph, or consolidation pass.

Writes are limited to surprising, harmful, relieving, or sufficiently novel events. Repeated
matching evidence increases support instead of consuming another slot. Nearby contradictory
outcomes are retained and marked. Retention decays deterministically with lifetime age. At
capacity the lowest retention/evidence/surprise record is evicted, with tick and event identity
as deterministic tie breakers.

Retrieval compares the current versioned latent and current health/fullness/pain drive, then
returns exactly four summaries (blank padded). Policy retrieval preserves distinct action
alternatives before filling spare top-k slots, compares evidence-weighted mean body outcomes,
and refuses a contradictory record until its matching evidence outnumbers contradictions.
Prediction retrieval filters by the exact action before top-k truncation and aggregates
consistent body residuals by median. It cannot overwrite vision or held-item prediction because
the key has no independently validated visual or held-item similarity metric. No usable match
falls back exactly to the recurrent prediction.

Work remains bounded by 64 records and four returned summaries per decision, and all state
remains private to one runtime. Prediction retrieval and action retrieval are independent
checkpointed switches, selected live with
`SIM_LUANTI_EPISODIC=off|prediction|policy|both`.

Checkpoint schema 4 adds this memory. Schemas 1–3 migrate to the canonical empty state.
Diagnostics expose only event count, configuration, and a fingerprint; records never cross the
bridge.

## Preregistered A/B

Run:

```text
SIM_LUANTI_SEED=101 SIM_LUANTI_AGENT_ID=4 cargo run -p sim-luanti -- \
  episodic-eval TRACE.ndjson docs/episodic-memory-result.json
```

Both arms receive the same genome, seed, ordered training prefix, actions, and observations.
Arm A is the completed recurrent predictor. Arm B is the identical predictor plus frozen
episodic retrieval. The first 80% trains both; the final 20% receives no writes. Prediction
success requires B to have strictly lower MAE than A for both vision and body.

The action fixture presents both arms with the same bounded body-accessible experiences of two
unnamed primitive motions in the same compact state: one repeatedly precedes injury and pain,
the other repeatedly precedes fullness relief. On 16 later no-write opportunities per each of
eight fixed seeds, success requires B to make fewer repeated injury choices and more
resource-return choices than A for every seed. This is a deterministic embodied policy-path
contract test, not evidence of transfer to uncontrolled live VoxeLibre behavior.

The broad episodic gate requires a fresh embodied policy benefit and either a prediction benefit
or a policy-only result that is independent of prediction retrieval and has no prediction
regression. An action fixture or prediction gain alone cannot pass it.

## Current result

[`episodic-memory-result.json`](episodic-memory-result.json) records the run. The only locally
available trace was the
186-transition earlier development trace (148 train, 38 held out), whose suffix had already been
examined while correcting the recurrent predictor. It is therefore diagnostic and is not fresh
gate evidence.

- Vision MAE: A 21.515, B 29.032; B had 8 transition wins, 9 ties, and 21 losses.
- Body MAE: A 4.614, B 4.504; B had 8 wins, 26 ties, and 4 losses.
- Held-item MAE: both zero on an unchanged empty hand.
- Controlled action fixture: A repeated 16 injuries and made zero resource-return choices per
  seed; B made zero injury choices and 16 resource-return choices per seed, for all eight seeds.

Episodic retrieval slightly improved body prediction but substantially worsened vision on this
diagnostic trace, so it did **not** pass the prediction criterion. It did improve the deliberately
controlled repeated-mistake fixture, but that is insufficient for the broader claim.
`prediction_success=false`, `action_success=true`, and
`episodic_gate_passes=false`.

## Diagnosis and correction

The visual regression had a structural cause. Retrieval ranked records by a 16-value latent plus
explicit health/fullness/pain drive, truncated all actions together, and then one nearest
same-action record replaced all 24 predicted residuals. Body had an explicit similarity signal;
vision did not. Contradictions and the other top-k records did not affect that replacement, and
unrelated actions could consume the top-k before same-action filtering. Encoder versions were
validated and the recurrent encoder is fixed, so stale/versioned keys were not the observed
cause.

The smallest correction is modality isolation plus conservative aggregation: exact-action
filtering occurs before top-k, only body features 12–17 can be amended, consistent records are
median-aggregated, and otherwise prediction falls back unchanged. Policy aggregation now keeps
action alternatives and accounts for evidence and contradictions. The two retrieval paths
remain independently switchable.

On the already-examined diagnostic trace, this changed vision from A 21.515 versus B 29.032 to
A 21.515 versus B 21.515: the regression is removed, not converted into a claimed gain. Body is
also an exact tie at 4.614 because no exact-action match exists; this demonstrates that
prediction retrieval is irrelevant for that trace.

## Fresh preregistered result

The protocol and fixed success rule were written to
[`episodic-live-ab-preregistration.json`](episodic-live-ab-preregistration.json) before outcome
generation. One invalid setup arm produced 17 rather than 16 trials and was excluded before its
paired B arm existed; the unchanged protocol was then run in real Luanti/VoxeLibre over seeds
211, 223, 227 and 229. The audit is in
[`episodic-live-ab-aborted-setup.json`](episodic-live-ab-aborted-setup.json).

Each valid arm used a fresh newborn, reset map, identical paired genome/runtime/world seed,
ordinary player damage and hunger, 16 no-override decisions, and prediction retrieval disabled.
A and B both made 64 injury choices and zero resource returns. There were zero positive and zero
negative pairs. The recurrent policy never sampled backward thrust, so memory never acquired a
beneficial return episode; the earlier synthetic result depended on supplying both outcomes in
advance.

Separately, the four previously untouched A traces were split 80/20 and evaluated with frozen
memory writes. Across 51 held-out transitions, corrected prediction retrieval had zero wins,
51 ties and zero losses for both vision and body. Thus the visual regression is corrected and
prediction is causally irrelevant to the policy-only live result, but there is no prediction
gain.

Newborn isolation, 64-record capacity/eviction, checkpoint restart and exact trace replay pass.
The bounded runtime path sustained 36,352 decisions/second in the local optimized benchmark,
well above the 10 Hz body rate. The GPU parity executable initially segfaulted only when four
hardware tests created Intel Meteor Lake Vulkan devices concurrently; every test passed alone.
Serializing device-owning tests (without changing any parity assertion) makes the full hardware
suite pass 4/4.

Machine-readable fresh evidence is in
[`episodic-memory-fresh-result.json`](episodic-memory-fresh-result.json). The behavioral success
rule was not met, so `episodic_gate_passes=false`. No graph, communication, semantic or spatial
work is opened.

## Full causal-chain follow-up

The original four seeds are consumed diagnostic data. Their zero effect was not weak score
integration or a short memory horizon. Baseline mode had no curiosity/homeostatic score
(`intrinsic_score=0`), episodic policy was a hard replacement, and all 64 fixture choices were
nonnegative thrust. The only relieving action was therefore absent from experience, so no
positive episode could be retrieved or selected. Exact one-step attribution itself was correct
for ordinary bridge actions, but the first replacement fixtures exposed two additional invalid
timing paths: a health reset was a second positive bodily consequence in v2, and an asynchronous
globalstep could apply v3's fullness change after construction of the intended observation.
Their preregistered seeds were consumed and never rerun; the audits are in
[`episodic-live-choice-v2-aborted.json`](episodic-live-choice-v2-aborted.json) and
[`episodic-live-choice-v3-aborted.json`](episodic-live-choice-v3-aborted.json).

The final v4 fixture was preregistered before outcome generation in
[`episodic-live-choice-v4-preregistration.json`](episodic-live-choice-v4-preregistration.json).
It uses the ordinary discrete curiosity policy, no action override or body reset, and two
repeated choices over the same VoxeLibre hunger store. A test-only pre-observation hook applies
the selected action's consequence after its full hold interval and immediately before
interoception, making the measured eligibility delay exactly one. Every A arm independently
sampled both directions during warmup and observed both positive and negative fullness changes.

Across untouched seeds 379, 383, 389, 397, 401 and 409, B reduced resource-loss choices from
121 to 22 and increased resource-return choices from 119 to 218. The paired return deltas were
`+22, +24, +20, +16, -5, +22`: five positive pairs and one negative pair. B recorded 541
episode insertions, including 51 positive and 38 negative body outcomes; 922 decisions retrieved
body evidence, 302 had a positive remembered score delta, and all 302 changed the selected
action. Baseline curiosity scores ranged from 55 to 2238 and episodic score deltas from 0 to
712, but these scales do not compete: a qualifying episodic alternative replaces the baseline
action. Fifteen changed actions were followed by a nonsaturated positive fullness delta; later
backward choices at maximum fullness correctly had zero immediate delta.

The preregistered fixture and mediation checks pass, but the behavioral gate requires no
negative pair. Seed 401 had only six episodic changes and moved from 25 to 20 returns. Consumed
trace replay showed that this was not top-k exclusion: whenever positive backward history
existed, backward thrust was represented among the four summaries. The failure was the
per-record `evidence > contradictions` veto. Exact latent/source reinforcement usually remained
one while one conflicting event incremented contradiction counters on several nearby records.
Useful positive outcomes at fullness 102, 153 and 204 were therefore discarded with evidence
one and contradiction counts one to four, even when the retrieved signed outcomes favored
backward thrust. Latent distances were small (typically 6–26 in these aligned contexts); drive
distance, not body/action misalignment, explained the larger margins when fullness differed.
This is a double-counted evidence-balance defect under context variation, not missing storage,
missing actions, distribution shift, or a reason to add graph/spatial memory. The machine-readable run is
[`../integrations/luanti/episodic-ab-runs/2026-10-22-choice-v4/episodic-live-choice-result.json`](../integrations/luanti/episodic-ab-runs/2026-10-22-choice-v4/episodic-live-choice-result.json).
The strict gate remains failed and no graph, spatial map, communication, or culture work is
opened. Final regression checks passed strict workspace Clippy, core CPU tests, the serialized
four-test hardware GPU parity suite, a 150-transition Luanti smoke/replay, and the real
VoxeLibre body/resume test with 183 felt observations. The bounded affordance path measured
35,169 decisions/second, remaining far above the 10 Hz body rate.

## Local-consensus follow-up

The smallest correction leaves retrieval, storage and both ablation switches unchanged. Policy
selection now aggregates the retrieved outcomes for each exact primitive action with evidence
weights. It acts only when positive support exceeds negative support, mean body utility is
positive, and that consensus exceeds the represented fallback; ties, negative consensus and
missing improvement abstain. Historical contradiction counters remain auditable but are not
counted as independent observations. This removes the pairwise double count while preserving
bounded four-summary work and explicit ambiguity abstention.

The v5 protocol, eight seeds and thresholds were frozen in
[`episodic-live-choice-v5-preregistration.json`](episodic-live-choice-v5-preregistration.json)
before generating data. Success required at least 16 aggregate fewer losses and more returns,
at least six positive pairs, no negative pair, complete mediation, prediction independence, and
non-regressive frozen prediction retrieval.

On untouched paired seeds 431, 433, 439, 443, 449, 457, 461 and 463, the policy criterion passed:
losses fell from 173 to 0, returns rose from 147 to 320, and pair deltas were
`+19, +30, +16, +21, +21, +19, +25, +22`. B inserted 547 episodes and reinforced 22, including
47 positive and 29 negative outcome writes. It retrieved body evidence 924 times, produced 509
positive score deltas and action changes, changed 472 actions to backward thrust, and 464 were
followed by a nonnegative delayed outcome. All retained eligibility delays were one. All sixteen
arms replayed exactly.

The independent untouched prediction check failed, however. Vision remained an exact tie over
740 held-out transitions (MAE 18.269 in both arms), but body prediction regressed from MAE 3.291
to 5.351, with 25 wins, 619 ties and 96 losses; seven of eight seed traces regressed. Prediction
retrieval was disabled in both live policy arms, so it did not mediate the behavioral result,
but the preregistered non-regression condition was still unmet. These seeds are consumed and
were not used for another correction or rerun.

Final checks passed newborn isolation, deterministic 64-event eviction, checkpoint restart,
all sixteen live trace replays, strict workspace Clippy, 72 core tests and the non-GUI workspace
suite, four hardware GPU/CPU parity tests, a 149-transition Luanti smoke/replay, and the real
VoxeLibre body/resume test with 185 felt observations. The bounded affordance path measured
35,225 decisions/second. The behavioral policy gate passes, but the full episodic-memory gate
remains **failed** because prediction regressed.
