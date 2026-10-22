# Compact lifetime recurrent predictor

Status: implemented baseline; held-out milestone result remains evidence-gated.

The runtime now maintains a per-individual 16-value recurrent latent state. A deterministic
genome/development-keyed gated reservoir consumes 24 nonsemantic sensory features plus eight
features of the action about to be applied. The sensory features comprise pooled eye/retina appearance and
motion, body/interoception, and held-item appearance and physical condition. No item name, label,
task reward, map, pretrained state, or communication channel enters the model.

The model predicts residual change from the current 24 sensory features, so a blank newborn starts
at the persistence baseline. Its fixed recurrent encoder is regenerated from inherited
development. A robust decoder takes the per-feature median residual of the nine nearest lifetime
recurrent states. The learned decoder state, optimizer state, hidden state, sensory anchor, pending
prediction, and 256-sample replay are acquired lifetime state. All are blank for a newborn and are
checkpointed for a continuing individual; reproduction does not copy them.

Online learning records one bounded residual example per healthy decision. An auxiliary parametric
diagnostic performs one deterministic eight-sample rehearsal batch every two decisions, while the
emitted prediction uses the robust replay decoder. Replay is bounded and the auxiliary rehearsal
evenly samples its ring to reduce rapid forgetting. The median residual matches the evaluation's
absolute-error objective and falls back exactly to persistence until nine examples exist. The
compact update currently remains on the request path because its fixed work is small; a larger
model or backpropagation through the recurrent encoder must move to a learner thread before being
enabled live.

Persistence schema 3 adds this state and migrates schemas 1–2 to a blank predictor. Bridge protocol
5 adds a recurrent-state fingerprint to diagnostics. Exact same-build replay includes the new
state.

## Preregistered evaluation

Run:

```text
cargo run -p sim-luanti -- prediction-eval TRACE.ndjson [REPORT.json]
```

`SIM_LUANTI_SEED` and `SIM_LUANTI_AGENT_ID` select the individual. The command trains on the first
80% of that individual's contiguous trace and freezes learned parameters for the final 20%. It
reports machine-readable mean absolute error for vision, body, and held-item modalities against
two fixed baselines: repeat the last observation and the frozen training-prefix mean. It also
reports feature sample counts and paired held-out transition wins, ties, and losses.

The milestone passes only if the fixed informative modalities, vision and body, each strictly beat
both baselines on the held-out suffix. Held-item physics remains reported but does not gate the
milestone: an always-empty hand has exactly zero baseline error and is therefore impossible to beat.
A tie never passes for a gating modality. Implementation and unit-test success alone do not count
as a pass.

## Diagnosis of the failed first trace

The preregistered command was run with seed 101 for individual 4 on the existing
`2026-10-22-body-four-rerun` trace: 148 training transitions and 38 held-out transitions.
MAE (recurrent / persistence / training mean) was:

- vision: 24.598 / 22.032 / 31.186
- body: 5.627 / 5.135 / 11.482
- held item: 0 / 0 / 0

The recurrent predictor beat the mean but not persistence for vision or body, and tied both on the
unchanged empty-hand channel.

Inspection found no evaluator leakage: the decoder was frozen over the suffix, both baselines used
only the training prefix or preceding observation, and 184 of 185 recorded action intervals matched
the preceding emitted action (the remaining interval was a transport edge). The runtime predictor
did have an off-by-one action-conditioning bug: it encoded the action that had produced the current
observation instead of the action about to produce the predicted observation. Its random projection
was also attenuated enough to quantize most latent signal away, the recurrent gate changed with an
arbitrary truncation phase despite the encoder being fixed, and the 32-sample replay forgot most of
the 148-sample prefix. Finally, squared-error-style decoder updates were a poor match for sparse
residuals judged by MAE: rare deaths and resets created nonzero biases that made persistence worse.
The update cadence and tiny retained window therefore underfit stable action effects while tracking
short nonstationary bursts too strongly. Cross-modality loss scaling was not the culprit because
each output had an independent decoder, but encoder quantization suppressed all modalities.
Continuous hidden state was carried correctly across the split; true TBPTT is not applicable while
the recurrent reservoir is fixed, and the old eight-step counter was bookkeeping rather than
recurrent training. Predicting absolute observations would be especially inappropriate here:
persistence is already strong, so the learnable target must remain the action-conditioned residual.

The correction conditions each prediction on the action about to be applied, encodes sensory
change rather than absolute level, removes truncation phase from inference, retains 256 bounded
examples, and uses nearest-state median residuals. The original trace was used only as a development
set after its suffix had been seen; its improved result (vision 21.515 versus 22.032 persistence,
body 4.614 versus 5.135) is not milestone evidence.

## Fresh preregistered result

Before collecting new data, the protocol was fixed to seed 1, natural individual 4 in a fresh
four-body body-contract run, and the same first-80%/final-20% split. The resulting untouched suffix
had 45 transitions after 180 training transitions:

- vision: 26.535 recurrent / 28.162 persistence / 26.555 training mean; 540 feature samples;
  paired against persistence 23 wins, 8 ties, 14 losses
- body: 2.470 / 2.529 / 9.107; 270 feature samples; 3 wins, 41 ties, 1 loss
- held item: 0 / 0 / 0; 270 feature samples; 45 ties

Vision and body both strictly beat both baselines, so the revised informative-modality gate passes.
The margins, especially vision versus the running mean and body versus persistence, are small; this
is passing evidence for the compact baseline, not strong statistical evidence of a large effect.
The machine-readable report is written to `target/lifetime-prediction-eval.json`.

The four-body collection completed all body abilities and wrote the complete trace, but its wrapper
later exited nonzero because it killed the restarted bridge as soon as the earlier `listening` line
appeared, racing the subsequent population-load confirmation. The wrapper now waits for that later
confirmation. A fresh four-body run passes all body checks and population resume, and a deterministic
four-founder test verifies each identity, body slot, predictor state, sequence, and next decision
across save/restart. The frozen trace and held-out metrics were not changed.
