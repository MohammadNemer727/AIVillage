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
| PerActionSituations | early | 8.8 | 12.6 | 17.7 | 17.7 |
| PerActionSituations | late | 11.0 | 15.5 | 24.7 | 24.7 |
| SceneSituations | early | 7.6 | 11.9 | 12.8 | 12.8 |
| SceneSituations | late | 10.0 | 14.6 | 21.1 | 21.1 |

## Late-phase hit rate per seed (%)

| controller | seeds |
|---|---|
| Random | 15.1 · 17.9 · 6.8 · 12.9 · 13.9 · 19.2 · 6.0 · 16.1 |
| AffordanceDiscovery | 11.9 · 25.2 · 7.0 · 16.0 · 18.6 · 0.0 · 7.2 · 4.3 |
| ContextDiscovery | 37.9 · 32.3 · 26.2 · 33.6 · 27.3 · 6.6 · 19.2 · 19.3 |
| ExpectileDiscovery | 17.8 · 21.1 · 11.1 · 16.6 · 30.3 · 14.8 · 17.2 · 14.2 |
| LearnedSituations | 16.6 · 33.3 · 20.9 · 15.3 · 19.7 · 14.8 · 13.9 · 13.7 |
| PerActionSituations | 31.4 · 37.1 · 0.0 · 31.7 · 32.5 · 21.0 · 21.0 · 12.6 |
| SceneSituations | 25.0 · 33.3 · 22.2 · 6.6 · 19.3 · 27.2 · 11.6 · 25.9 |

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
| 7 | near left, near behind, touch |
| 19 | near left, near right, near ahead |
| 43 | near ahead |
| 91 | near right, near ahead, touch |
| 131 | near behind, near right, touch, near ahead |
| 173 | near right, near ahead |
| 211 | near behind, looking up, near ahead |
| 251 | near left, near behind, near right |

## SceneSituations: the push's own situations over learned scenes

Sense facts each seed split on, in order.

| seed | facts |
|---:|---|
| 7 | scene 3, scene 5 |
| 19 | scene 3, scene 5 |
| 43 | scene 6, scene 5 |
| 91 | scene 3, scene 4, scene 7, scene 1 |
| 131 | scene 4, scene 3, scene 1, scene 0 |
| 173 | scene 2 |
| 211 | scene 5, scene 7, scene 1 |
| 251 | scene 0, scene 5, scene 7 |
