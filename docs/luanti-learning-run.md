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
| free | baseline | 800 | 7.9 | 0.0 | 0.0 | 24.5 |
| free | connected | 1600 | 16.0 | 2.9 | 3.9 | 19.5 |
| free | extinction | 800 | 8.0 | 0.0 | 0.0 | 22.4 |
| inert | baseline | 800 | 7.1 | 0.0 | 0.0 | 24.2 |
| inert | connected | 1600 | 7.7 | 0.0 | 0.0 | 33.4 |
| inert | extinction | 800 | 7.6 | 0.0 | 0.0 | 25.1 |

## Per run (hand pulses per 100 decisions)

| run | condition | seed | baseline | connected | extinction |
|---|---|---:|---:|---:|---:|
| free-seed1 | free | 1 | 5.5 | 15.2 | 5.5 |
| free-seed2 | free | 2 | 9.5 | 17.0 | 8.0 |
| free-seed3 | free | 3 | 11.0 | 15.5 | 8.5 |
| free-seed4 | free | 4 | 5.5 | 16.2 | 10.0 |
| inert-seed1 | inert | 1 | 7.0 | 6.2 | 6.0 |
| inert-seed2 | inert | 2 | 8.0 | 7.0 | 8.0 |
| inert-seed3 | inert | 3 | 7.0 | 8.2 | 8.0 |
| inert-seed4 | inert | 4 | 6.5 | 9.2 | 8.5 |
