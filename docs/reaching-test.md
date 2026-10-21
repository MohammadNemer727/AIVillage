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

## Late-phase hit rate per seed (%)

| controller | seeds |
|---|---|
| Random | 15.1 · 17.9 · 6.8 · 12.9 · 13.9 · 19.2 · 6.0 · 16.1 |
| AffordanceDiscovery | 11.9 · 25.2 · 7.0 · 16.0 · 18.6 · 0.0 · 7.2 · 4.3 |
| ContextDiscovery | 37.9 · 32.3 · 26.2 · 33.6 · 27.3 · 6.6 · 19.2 · 19.3 |
