# Reaching test

The body turns between 8 directions and a reachable object stands at eye height in one of them; a hand pulse reaches it only while the founder faces it. Each 40-decision trial starts facing a seed-chosen direction. Over 8 seeds, 1000 decisions per phase.

*Facing* is the share of decisions spent facing the object; *pushes while facing* is the share of pushes made then. Pushing on purpose shows as pushes while facing well above facing. *Hit rate* is the share of pushes that reached the object.

| controller | phase | pushes per 100 | facing % | pushes while facing % | hit rate % |
|---|---|---:|---:|---:|---:|
| Random | early | 8.5 | 14.2 | 14.5 | 14.5 |
| Random | late | 7.8 | 12.0 | 13.5 | 13.5 |
| AffordanceDiscovery | early | 7.6 | 12.1 | 15.6 | 15.6 |
| AffordanceDiscovery | late | 7.8 | 13.6 | 12.6 | 12.6 |
| ContextDiscovery | early | 8.7 | 13.6 | 19.6 | 19.6 |
| ContextDiscovery | late | 9.7 | 14.0 | 26.4 | 26.4 |
| ExpectileDiscovery | early | 7.6 | 10.8 | 10.4 | 10.4 |
| ExpectileDiscovery | late | 8.7 | 13.6 | 18.3 | 18.3 |
| LearnedSituations | early | 8.0 | 13.1 | 17.6 | 17.6 |
| LearnedSituations | late | 8.4 | 13.9 | 18.5 | 18.5 |
| PerActionSituations | early | 8.3 | 12.9 | 12.5 | 12.5 |
| PerActionSituations | late | 10.0 | 13.2 | 20.2 | 20.2 |
| SceneSituations | early | 8.0 | 11.5 | 14.4 | 14.4 |
| SceneSituations | late | 9.5 | 15.1 | 22.4 | 22.4 |
| ScenePlanner | early | 9.2 | 17.8 | 29.1 | 29.1 |
| ScenePlanner | late | 9.3 | 20.1 | 33.8 | 33.8 |
| FovealPlanner | early | 7.6 | 12.3 | 19.3 | 19.3 |
| FovealPlanner | late | 8.2 | 16.9 | 31.7 | 31.7 |
| SalientPlanner | early | 9.3 | 16.9 | 30.5 | 30.5 |
| SalientPlanner | late | 9.0 | 18.3 | 28.0 | 28.0 |

## Late-phase hit rate per seed (%)

| controller | seeds |
|---|---|
| Random | 15.1 · 17.9 · 6.8 · 12.9 · 13.9 · 19.2 · 6.0 · 16.1 |
| AffordanceDiscovery | 11.9 · 25.2 · 7.0 · 16.0 · 18.6 · 0.0 · 7.2 · 4.3 |
| ContextDiscovery | 37.9 · 32.3 · 26.2 · 33.6 · 27.3 · 6.6 · 19.2 · 19.3 |
| ExpectileDiscovery | 17.8 · 21.1 · 11.1 · 16.6 · 30.3 · 14.8 · 17.2 · 14.2 |
| LearnedSituations | 16.6 · 33.3 · 20.9 · 15.3 · 19.7 · 14.8 · 13.9 · 13.7 |
| PerActionSituations | 29.3 · 36.8 · 15.6 · 34.6 · 10.4 · 5.2 · 16.6 · 3.7 |
| SceneSituations | 34.3 · 28.1 · 0.0 · 28.5 · 12.1 · 13.2 · 34.4 · 20.9 |
| ScenePlanner | 13.7 · 47.5 · 50.8 · 43.3 · 33.3 · 26.8 · 26.3 · 0.0 |
| FovealPlanner | 39.7 · 30.5 · 0.0 · 40.6 · 39.7 · 13.4 · 56.3 · 11.6 |
| SalientPlanner | 23.0 · 33.3 · 31.1 · 27.4 · 34.2 · 23.8 · 28.7 · 19.0 |

## LearnedSituations: shared situations

Sense facts each seed split on, in order.

| seed | facts |
|---:|---|
| 7 | in the air, near left, near right, near ahead |
| 19 | in the air, near behind, near left, near right, near right, near ahead |
| 43 | near behind, looking up, near ahead, near left, near right |
| 91 | near right, in the air, near behind |
| 131 | in the air, looking down, near behind, near right, near ahead, near right |
| 173 | near left, looking up, in the air, near ahead, near ahead, in the air |
| 211 | in the air, near right, near behind, looking up, near ahead, near ahead |
| 251 | near left, near behind, in the air, near behind |

## PerActionSituations: the push's own situations

Sense facts each seed split on, in order.

| seed | facts |
|---:|---|
| 7 | near right, touch, near left |
| 19 | near behind, near right, touch |
| 43 | near ahead, near right |
| 91 | near right, near ahead |
| 131 | near behind, near ahead |
| 173 | near behind, near right |
| 211 | near left, near right, looking down |
| 251 | near ahead |

## SceneSituations: the push's own situations over learned scenes

Sense facts each seed split on, in order.

| seed | facts |
|---:|---|
| 7 | scene 3, scene 6 |
| 19 | scene 7, scene 2 |
| 43 | none |
| 91 | scene 0, scene 1, scene 3 |
| 131 | scene 0, scene 5, scene 6 |
| 173 | scene 4, scene 6, scene 3 |
| 211 | scene 2, scene 3 |
| 251 | scene 3, scene 5 |
