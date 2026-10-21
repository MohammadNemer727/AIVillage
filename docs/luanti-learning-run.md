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
| free / context / toward | baseline | 800 | 7.9 | 0.0 | 0.0 | 0.0 | 25.4 |
| free / context / toward | connected | 4000 | 11.3 | 45.5 | 42.0 | 44.2 | 26.4 |
| free / context / toward | extinction | 800 | 10.0 | 0.0 | 0.2 | 0.0 | 32.5 |
| free / context / turned | baseline | 800 | 8.6 | 0.0 | 0.0 | 0.0 | 27.4 |
| free / context / turned | connected | 4000 | 9.6 | 20.6 | 10.3 | 14.8 | 29.1 |
| free / context / turned | extinction | 800 | 8.2 | 0.0 | 0.0 | 0.0 | 31.8 |
| free / default / toward | baseline | 800 | 7.5 | 0.0 | 0.0 | 0.0 | 28.1 |
| free / default / toward | connected | 4000 | 15.1 | 56.4 | 47.6 | 50.7 | 24.9 |
| free / default / toward | extinction | 800 | 8.2 | 0.0 | 0.1 | 0.0 | 12.5 |
| free / default / turned | baseline | 800 | 7.5 | 0.0 | 0.0 | 0.0 | 25.1 |
| free / default / turned | connected | 4000 | 11.8 | 29.5 | 15.1 | 21.1 | 25.3 |
| free / default / turned | extinction | 800 | 7.0 | 0.0 | 0.1 | 0.0 | 14.9 |
| free / foveal-planner / turned | baseline | 800 | 6.8 | 0.0 | 0.0 | 0.0 | 21.4 |
| free / foveal-planner / turned | connected | 4000 | 10.6 | 24.6 | 15.2 | 15.4 | 23.5 |
| free / foveal-planner / turned | extinction | 800 | 9.2 | 0.0 | 0.2 | 0.0 | 37.8 |
| free / learned / turned | baseline | 800 | 6.4 | 0.0 | 0.0 | 0.0 | 17.8 |
| free / learned / turned | connected | 4000 | 9.7 | 25.1 | 13.9 | 15.2 | 25.4 |
| free / learned / turned | extinction | 800 | 6.8 | 0.0 | 0.1 | 0.0 | 22.4 |
| free / per-action-8 / turned | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 22.0 |
| free / per-action-8 / turned | connected | 4000 | 11.9 | 20.8 | 13.7 | 11.7 | 32.5 |
| free / per-action-8 / turned | extinction | 800 | 9.8 | 0.0 | 0.1 | 0.0 | 27.5 |
| free / planner-32 / turned | baseline | 800 | 6.8 | 0.0 | 0.0 | 0.0 | 20.9 |
| free / planner-32 / turned | connected | 4000 | 11.4 | 26.8 | 16.9 | 16.9 | 31.3 |
| free / planner-32 / turned | extinction | 800 | 11.2 | 0.0 | 0.2 | 0.0 | 29.5 |
| free / scenes-slowfade / turned | baseline | 800 | 6.8 | 0.0 | 0.0 | 0.0 | 20.6 |
| free / scenes-slowfade / turned | connected | 4000 | 11.9 | 30.6 | 17.1 | 21.4 | 25.9 |
| free / scenes-slowfade / turned | extinction | 800 | 10.9 | 0.0 | 0.2 | 0.0 | 31.1 |
| inert / context / toward | baseline | 800 | 8.6 | 0.0 | 0.0 | 0.0 | 22.5 |
| inert / context / toward | connected | 4000 | 8.0 | 0.0 | 39.6 | 36.5 | 34.7 |
| inert / context / toward | extinction | 800 | 7.9 | 0.0 | 0.1 | 0.0 | 31.5 |
| inert / context / turned | baseline | 800 | 8.6 | 0.0 | 0.0 | 0.0 | 27.0 |
| inert / context / turned | connected | 4000 | 7.0 | 0.0 | 17.3 | 15.6 | 27.9 |
| inert / context / turned | extinction | 800 | 9.4 | 0.0 | 0.4 | 0.0 | 24.4 |
| inert / default / toward | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 27.1 |
| inert / default / toward | connected | 4000 | 8.0 | 0.0 | 50.0 | 53.9 | 30.3 |
| inert / default / toward | extinction | 800 | 7.9 | 0.0 | 0.2 | 0.0 | 18.2 |
| inert / default / turned | baseline | 800 | 6.8 | 0.0 | 0.0 | 0.0 | 24.2 |
| inert / default / turned | connected | 4000 | 7.7 | 0.0 | 16.0 | 14.7 | 22.3 |
| inert / default / turned | extinction | 800 | 8.1 | 0.0 | 0.1 | 0.0 | 25.1 |
| inert / foveal-planner / turned | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 20.6 |
| inert / foveal-planner / turned | connected | 4000 | 7.5 | 0.0 | 16.0 | 12.3 | 25.8 |
| inert / foveal-planner / turned | extinction | 800 | 7.5 | 0.0 | 0.0 | 0.0 | 18.0 |
| inert / learned / turned | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 17.6 |
| inert / learned / turned | connected | 4000 | 7.8 | 0.0 | 14.4 | 13.8 | 25.3 |
| inert / learned / turned | extinction | 800 | 8.6 | 0.0 | 0.1 | 0.0 | 33.8 |
| inert / per-action-8 / turned | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 23.2 |
| inert / per-action-8 / turned | connected | 4000 | 8.0 | 0.0 | 14.9 | 19.6 | 28.8 |
| inert / per-action-8 / turned | extinction | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 25.5 |
| inert / planner-32 / turned | baseline | 800 | 7.1 | 0.0 | 0.0 | 0.0 | 24.6 |
| inert / planner-32 / turned | connected | 4000 | 7.5 | 0.0 | 16.8 | 13.3 | 29.6 |
| inert / planner-32 / turned | extinction | 800 | 7.0 | 0.0 | 0.0 | 0.0 | 23.5 |
| inert / scenes-slowfade / turned | baseline | 800 | 7.4 | 0.0 | 0.0 | 0.0 | 20.6 |
| inert / scenes-slowfade / turned | connected | 4000 | 7.4 | 0.0 | 21.6 | 25.4 | 27.4 |
| inert / scenes-slowfade / turned | extinction | 800 | 6.8 | 0.0 | 0.1 | 0.0 | 24.6 |

## Per run (hand pulses per 100 decisions)

| run | condition / learner / facing | seed | baseline | connected | extinction |
|---|---|---:|---:|---:|---:|
| context-free-seed1 | free / context / toward | 1 | 6.0 | 11.4 | 9.0 |
| context-free-seed2 | free / context / toward | 2 | 9.0 | 12.0 | 9.0 |
| context-free-seed3 | free / context / toward | 3 | 9.0 | 11.1 | 10.5 |
| context-free-seed4 | free / context / toward | 4 | 7.5 | 10.8 | 11.5 |
| context-inert-seed1 | inert / context / toward | 1 | 7.5 | 8.1 | 7.5 |
| context-inert-seed2 | inert / context / toward | 2 | 10.5 | 7.5 | 9.0 |
| context-inert-seed3 | inert / context / toward | 3 | 9.0 | 8.7 | 4.5 |
| context-inert-seed4 | inert / context / toward | 4 | 7.5 | 7.5 | 10.5 |
| default-free-seed1 | free / default / toward | 1 | 9.0 | 12.9 | 7.5 |
| default-free-seed2 | free / default / toward | 2 | 7.5 | 15.9 | 6.0 |
| default-free-seed3 | free / default / toward | 3 | 7.5 | 16.2 | 10.5 |
| default-free-seed4 | free / default / toward | 4 | 6.0 | 15.3 | 9.0 |
| default-inert-seed1 | inert / default / toward | 1 | 6.0 | 7.5 | 6.0 |
| default-inert-seed2 | inert / default / toward | 2 | 7.5 | 8.4 | 7.5 |
| default-inert-seed3 | inert / default / toward | 3 | 9.0 | 8.1 | 10.5 |
| default-inert-seed4 | inert / default / toward | 4 | 6.0 | 8.1 | 7.5 |
| context-free-seed1 | free / context / turned | 1 | 7.5 | 9.3 | 7.5 |
| context-free-seed2 | free / context / turned | 2 | 10.5 | 8.4 | 7.5 |
| context-free-seed3 | free / context / turned | 3 | 9.0 | 9.9 | 10.5 |
| context-free-seed4 | free / context / turned | 4 | 7.5 | 10.8 | 7.5 |
| context-inert-seed1 | inert / context / turned | 1 | 7.5 | 7.2 | 9.0 |
| context-inert-seed2 | inert / context / turned | 2 | 10.5 | 6.9 | 9.0 |
| context-inert-seed3 | inert / context / turned | 3 | 9.0 | 6.9 | 10.5 |
| context-inert-seed4 | inert / context / turned | 4 | 7.5 | 7.2 | 9.0 |
| default-free-seed1 | free / default / turned | 1 | 7.5 | 10.2 | 7.5 |
| default-free-seed2 | free / default / turned | 2 | 7.5 | 12.3 | 6.0 |
| default-free-seed3 | free / default / turned | 3 | 7.5 | 12.6 | 7.5 |
| default-free-seed4 | free / default / turned | 4 | 7.5 | 12.3 | 7.0 |
| default-inert-seed1 | inert / default / turned | 1 | 7.5 | 6.3 | 6.0 |
| default-inert-seed2 | inert / default / turned | 2 | 7.5 | 7.5 | 6.0 |
| default-inert-seed3 | inert / default / turned | 3 | 6.0 | 9.0 | 10.5 |
| default-inert-seed4 | inert / default / turned | 4 | 6.0 | 7.8 | 10.0 |
| learned-free-seed1 | free / learned / turned | 1 | 6.0 | 9.0 | 7.5 |
| learned-free-seed2 | free / learned / turned | 2 | 6.0 | 10.8 | 6.0 |
| learned-free-seed3 | free / learned / turned | 3 | 7.5 | 9.3 | 7.5 |
| learned-free-seed4 | free / learned / turned | 4 | 6.0 | 9.6 | 6.0 |
| learned-inert-seed1 | inert / learned / turned | 1 | 9.0 | 7.2 | 9.0 |
| learned-inert-seed2 | inert / learned / turned | 2 | 6.0 | 7.8 | 7.5 |
| learned-inert-seed3 | inert / learned / turned | 3 | 7.5 | 8.1 | 9.0 |
| learned-inert-seed4 | inert / learned / turned | 4 | 6.0 | 8.1 | 9.0 |
| pa-free-seed1 | free / per-action-8 / turned | 1 | 7.5 | 9.6 | 7.5 |
| pa-free-seed2 | free / per-action-8 / turned | 2 | 6.0 | 9.6 | 6.0 |
| pa-free-seed3 | free / per-action-8 / turned | 3 | 7.5 | 12.9 | 12.0 |
| pa-free-seed4 | free / per-action-8 / turned | 4 | 7.5 | 15.6 | 13.5 |
| pa-inert-seed1 | inert / per-action-8 / turned | 1 | 9.0 | 10.2 | 9.0 |
| pa-inert-seed2 | inert / per-action-8 / turned | 2 | 6.0 | 5.7 | 6.0 |
| pa-inert-seed3 | inert / per-action-8 / turned | 3 | 6.0 | 8.4 | 7.5 |
| pa-inert-seed4 | inert / per-action-8 / turned | 4 | 7.5 | 7.8 | 6.0 |
| ss-free-seed1 | free / scenes-slowfade / turned | 1 | 6.0 | 12.9 | 10.5 |
| ss-free-seed2 | free / scenes-slowfade / turned | 2 | 7.5 | 12.3 | 10.5 |
| ss-free-seed3 | free / scenes-slowfade / turned | 3 | 6.0 | 11.1 | 10.5 |
| ss-free-seed4 | free / scenes-slowfade / turned | 4 | 7.5 | 11.4 | 12.0 |
| ss-inert-seed1 | inert / scenes-slowfade / turned | 1 | 7.5 | 6.6 | 6.0 |
| ss-inert-seed2 | inert / scenes-slowfade / turned | 2 | 7.0 | 7.3 | 7.5 |
| ss-inert-seed3 | inert / scenes-slowfade / turned | 3 | 7.5 | 8.7 | 7.5 |
| ss-inert-seed4 | inert / scenes-slowfade / turned | 4 | 7.5 | 6.9 | 6.0 |
| p32-free-seed1 | free / planner-32 / turned | 1 | 7.5 | 10.8 | 14.0 |
| p32-free-seed2 | free / planner-32 / turned | 2 | 7.5 | 11.6 | 11.5 |
| p32-free-seed3 | free / planner-32 / turned | 3 | 6.0 | 12.9 | 8.0 |
| p32-free-seed4 | free / planner-32 / turned | 4 | 6.0 | 10.2 | 11.5 |
| p32-inert-seed1 | inert / planner-32 / turned | 1 | 7.5 | 5.5 | 6.5 |
| p32-inert-seed2 | inert / planner-32 / turned | 2 | 7.5 | 7.9 | 4.5 |
| p32-inert-seed3 | inert / planner-32 / turned | 3 | 7.5 | 8.3 | 9.5 |
| p32-inert-seed4 | inert / planner-32 / turned | 4 | 6.0 | 8.4 | 7.5 |
| fp-free-seed1 | free / foveal-planner / turned | 1 | 6.0 | 10.5 | 10.0 |
| fp-free-seed2 | free / foveal-planner / turned | 2 | 7.5 | 10.9 | 7.5 |
| fp-free-seed3 | free / foveal-planner / turned | 3 | 6.0 | 10.9 | 7.5 |
| fp-free-seed4 | free / foveal-planner / turned | 4 | 7.5 | 10.0 | 12.0 |
| fp-inert-seed1 | inert / foveal-planner / turned | 1 | 7.5 | 6.2 | 7.5 |
| fp-inert-seed2 | inert / foveal-planner / turned | 2 | 6.0 | 8.3 | 8.0 |
| fp-inert-seed3 | inert / foveal-planner / turned | 3 | 7.5 | 7.8 | 9.5 |
| fp-inert-seed4 | inert / foveal-planner / turned | 4 | 7.5 | 7.7 | 5.0 |
