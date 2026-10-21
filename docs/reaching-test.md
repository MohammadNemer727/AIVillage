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
| LearnedSituations | early | 8.0 | 12.4 | 16.7 | 16.7 |
| LearnedSituations | late | 8.6 | 13.9 | 20.8 | 20.8 |

## Late-phase hit rate per seed (%)

| controller | seeds |
|---|---|
| Random | 15.1 · 17.9 · 6.8 · 12.9 · 13.9 · 19.2 · 6.0 · 16.1 |
| AffordanceDiscovery | 11.9 · 25.2 · 7.0 · 16.0 · 18.6 · 0.0 · 7.2 · 4.3 |
| ContextDiscovery | 37.9 · 32.3 · 26.2 · 33.6 · 27.3 · 6.6 · 19.2 · 19.3 |
| ExpectileDiscovery | 17.8 · 21.1 · 11.1 · 16.6 · 30.3 · 14.8 · 17.2 · 14.2 |
| LearnedSituations | 24.1 · 35.4 · 10.2 · 17.2 · 26.7 · 11.1 · 22.2 · 17.2 |

## Situations LearnedSituations chose

Sense facts each seed split on, in order.

| seed | facts |
|---:|---|
| 7 | in the air, near right, near behind, near ahead, looking up, near left, near left, near left |
| 19 | near right, in the air, near left, in the air, looking down, near ahead, near ahead, near left, near behind, near behind, near ahead |
| 43 | in the air, near left, looking up, near ahead, near behind, near behind, near right, near ahead, near right |
| 91 | near left, near ahead, near behind, near right, near right, looking up, near ahead, near behind |
| 131 | near ahead, in the air, near behind, near left, looking down, near right, near left, near left, looking up |
| 173 | near ahead, near behind, looking up, looking up, near right, near left, near left, near right, near left |
| 211 | in the air, near right, near ahead, near ahead, near behind, near left, near behind, looking up, near ahead, near left |
| 251 | near left, near ahead, in the air, near behind, near ahead, near right, near right |
