# Luanti learning run

Real Luanti runs of the affordance founder in the infant mobile paradigm: baseline
without the test cube, a connected phase with it in front of the founder, then
extinction without it. The founder and cube are returned to their places every trial.
In the **free** condition a hand pulse within reach pushes the cube and reports contact;
in the **inert** control the cube is present and solid but a pulse does nothing.
Rates are per 100 decisions. Luanti physics is not bit-exact, so runs are not
reproducible move for move, but every trace replays exactly through the Rust runtime.

Reproduce one run, then summarise any set of runs:

```sh
SIM_LUANTI_SEED=1 LEARN_OBJECT=free sh integrations/luanti/ci/learning-run.sh /tmp/learn/free-seed1
python3 integrations/luanti/tools/analyze_learning.py docs/luanti-learning-run.md /tmp/learn/*
```

*Hit rate* is the share of hand pulses followed by contact. *Cube ahead* is the share
of decisions with the cube on the eye rays nearest straight ahead; *pulses while ahead*
is the share of pulses made then. Aiming shows as pulses while ahead above cube ahead.

## Pooled by condition and learner

| condition / learner / facing | phase | decisions | hand pulses | hit rate % | cube ahead % | pulses while ahead % | looking down % |
|---|---|---:|---:|---:|---:|---:|---:|
| free / default / toward | baseline | 800 | 7.5 | 0.0 | 0.0 | 0.0 | 28.1 |
| free / default / toward | connected | 4000 | 15.1 | 56.4 | 47.6 | 50.7 | 24.9 |
| free / default / toward | extinction | 800 | 8.2 | 0.0 | 0.1 | 0.0 | 12.5 |
| free / default / turned | baseline | 800 | 7.5 | 0.0 | 0.0 | 0.0 | 25.1 |
| free / default / turned | connected | 4000 | 11.8 | 29.5 | 15.1 | 21.1 | 25.3 |
| free / default / turned | extinction | 800 | 7.0 | 0.0 | 0.1 | 0.0 | 14.9 |
| inert / default / toward | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 27.1 |
| inert / default / toward | connected | 4000 | 8.0 | 0.0 | 50.0 | 53.9 | 30.3 |
| inert / default / toward | extinction | 800 | 7.9 | 0.0 | 0.2 | 0.0 | 18.2 |
| inert / default / turned | baseline | 800 | 6.8 | 0.0 | 0.0 | 0.0 | 24.2 |
| inert / default / turned | connected | 4000 | 7.7 | 0.0 | 16.0 | 14.7 | 22.3 |
| inert / default / turned | extinction | 800 | 8.1 | 0.0 | 0.1 | 0.0 | 25.1 |

## Per run (hand pulses per 100 decisions)

| run | condition / learner / facing | seed | baseline | connected | extinction |
|---|---|---:|---:|---:|---:|
| default-free-seed1 | free / default / toward | 1 | 9.0 | 12.9 | 7.5 |
| default-free-seed2 | free / default / toward | 2 | 7.5 | 15.9 | 6.0 |
| default-free-seed3 | free / default / toward | 3 | 7.5 | 16.2 | 10.5 |
| default-free-seed4 | free / default / toward | 4 | 6.0 | 15.3 | 9.0 |
| default-inert-seed1 | inert / default / toward | 1 | 6.0 | 7.5 | 6.0 |
| default-inert-seed2 | inert / default / toward | 2 | 7.5 | 8.4 | 7.5 |
| default-inert-seed3 | inert / default / toward | 3 | 9.0 | 8.1 | 10.5 |
| default-inert-seed4 | inert / default / toward | 4 | 6.0 | 8.1 | 7.5 |
| default-free-seed1 | free / default / turned | 1 | 7.5 | 10.2 | 7.5 |
| default-free-seed2 | free / default / turned | 2 | 7.5 | 12.3 | 6.0 |
| default-free-seed3 | free / default / turned | 3 | 7.5 | 12.6 | 7.5 |
| default-free-seed4 | free / default / turned | 4 | 7.5 | 12.3 | 7.0 |
| default-inert-seed1 | inert / default / turned | 1 | 7.5 | 6.3 | 6.0 |
| default-inert-seed2 | inert / default / turned | 2 | 7.5 | 7.5 | 6.0 |
| default-inert-seed3 | inert / default / turned | 3 | 6.0 | 9.0 | 10.5 |
| default-inert-seed4 | inert / default / turned | 4 | 6.0 | 7.8 | 10.0 |
