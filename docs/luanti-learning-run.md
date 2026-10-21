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

## Pooled by condition

| condition | phase | decisions | hand pulses | contacts | cube in view | looking down |
|---|---|---:|---:|---:|---:|---:|
| free | baseline | 800 | 6.8 | 0.0 | 0.0 | 30.0 |
| free | connected | 1600 | 15.4 | 4.8 | 2.4 | 15.4 |
| free | extinction | 800 | 8.1 | 0.0 | 0.0 | 16.4 |
| inert | baseline | 800 | 6.8 | 0.0 | 0.0 | 30.0 |
| inert | connected | 1600 | 6.9 | 0.0 | 0.7 | 19.5 |
| inert | extinction | 800 | 7.9 | 0.0 | 0.0 | 23.0 |

## Per run (hand pulses per 100 decisions)

| run | condition | seed | baseline | connected | extinction |
|---|---|---:|---:|---:|---:|
| free-seed1 | free | 1 | 6.0 | 12.8 | 7.5 |
| free-seed2 | free | 2 | 7.5 | 15.8 | 8.5 |
| free-seed3 | free | 3 | 7.5 | 17.2 | 9.0 |
| free-seed4 | free | 4 | 6.0 | 15.8 | 7.5 |
| inert-seed1 | inert | 1 | 6.0 | 6.8 | 7.5 |
| inert-seed2 | inert | 2 | 7.5 | 6.8 | 9.0 |
| inert-seed3 | inert | 3 | 7.5 | 6.8 | 7.5 |
| inert-seed4 | inert | 4 | 6.0 | 7.5 | 7.5 |
