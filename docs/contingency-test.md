# Contingency test

Infant mobile paradigm on deterministic synthetic fixtures over 8 seeds, 96 decisions per phase. In the contingent condition a hand pulse changes the next contact; in the yoked condition contact replays another seed's contingent run regardless of action. Standard: a pulse produces contact. Inverted: a pulse removes constant contact. Bimodal: a pulse turns half contact into full or none by a fair coin, leaving the average unchanged. No controller receives a reward.

## Hand pulses per 100 decisions

Anticipation is the learner's expected contact after a hand pulse, as distance from rest (0–255), averaged per phase before each outcome arrived. Consequences counts middle-phase decisions whose contact differed from rest.

| paradigm | controller | condition | baseline | middle | early extinction | late extinction | consequences | anticipation |
|---|---|---|---:|---:|---:|---:|---:|---|
| Standard | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Standard | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Standard | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Standard | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Standard | AffordanceDiscovery | Contingent | 3.6 | 18.0 | 20.5 | 10.1 | 139 | 0 → 132 → 117 → 44 |
| Standard | AffordanceDiscovery | Yoked | 3.6 | 3.6 | 4.9 | 4.4 | 139 | 0 → 10 → 23 → 16 |
| Inverted | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | AffordanceDiscovery | Contingent | 3.6 | 18.0 | 20.5 | 10.1 | 139 | 0 → 132 → 118 → 45 |
| Inverted | AffordanceDiscovery | Yoked | 3.6 | 3.6 | 4.9 | 4.4 | 139 | 0 → 10 → 23 → 16 |
| Bimodal | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | AffordanceDiscovery | Contingent | 3.6 | 4.8 | 4.9 | 4.9 | 37 | 0 → 16 → 20 → 14 |
| Bimodal | AffordanceDiscovery | Yoked | 3.6 | 3.7 | 2.8 | 5.2 | 37 | 0 → 2 → 7 → 5 |

## Verdicts

A run shows contingency when connected-phase pulses are at least 1.5× both baseline and the yoked middle phase, and their rate falls below the connected rate by late extinction. Early extinction is not judged, so an extinction burst does not count against learning.

| paradigm | controller | pooled | seeds passing |
|---|---|---|---:|
| Standard | PredictiveBaseline | false | 0/8 |
| Standard | Random | false | 0/8 |
| Standard | AffordanceDiscovery | true | 8/8 |
| Inverted | PredictiveBaseline | false | 0/8 |
| Inverted | Random | false | 0/8 |
| Inverted | AffordanceDiscovery | true | 8/8 |
| Bimodal | PredictiveBaseline | false | 0/8 |
| Bimodal | Random | false | 0/8 |
| Bimodal | AffordanceDiscovery | false | 2/8 |

## AffordanceDiscovery learning curves

Hand pulses per 24 decisions for each seed, phases separated by `|`.

| paradigm | seed | contingent | yoked | passes |
|---|---:|---|---|---|
| Standard | 7 | 2 0 1 1 | 1 3 4 8 | 5 4 4 2 | 2 0 1 1 | 1 1 1 1 | 0 1 1 2 | true |
| Standard | 19 | 1 1 1 1 | 2 4 7 6 | 8 2 3 3 | 1 1 1 1 | 1 1 1 0 | 2 1 1 1 | true |
| Standard | 43 | 1 1 1 0 | 4 5 7 6 | 6 3 3 3 | 1 1 1 0 | 1 1 2 1 | 2 1 1 1 | true |
| Standard | 91 | 1 1 1 0 | 1 5 6 6 | 6 4 3 1 | 1 1 1 0 | 1 1 0 1 | 1 2 2 1 | true |
| Standard | 131 | 1 1 0 1 | 2 4 5 7 | 5 5 2 2 | 1 1 0 1 | 1 1 1 1 | 1 1 0 1 | true |
| Standard | 173 | 2 1 0 1 | 0 3 3 6 | 6 4 3 2 | 2 1 0 1 | 0 2 0 1 | 1 2 1 1 | true |
| Standard | 211 | 2 1 0 1 | 1 5 5 6 | 8 2 3 1 | 2 1 0 1 | 1 1 0 1 | 1 0 2 1 | true |
| Standard | 251 | 1 1 1 0 | 1 3 7 6 | 7 4 1 3 | 1 1 1 0 | 1 0 1 1 | 2 1 0 1 | true |
| Inverted | 7 | 2 0 1 1 | 1 3 4 8 | 5 4 4 2 | 2 0 1 1 | 1 1 1 1 | 0 1 1 2 | true |
| Inverted | 19 | 1 1 1 1 | 2 4 7 6 | 8 2 3 3 | 1 1 1 1 | 1 1 1 0 | 2 1 1 1 | true |
| Inverted | 43 | 1 1 1 0 | 4 5 7 6 | 6 3 3 3 | 1 1 1 0 | 1 1 2 1 | 2 1 1 1 | true |
| Inverted | 91 | 1 1 1 0 | 1 5 6 6 | 6 4 3 1 | 1 1 1 0 | 1 1 0 1 | 1 2 2 1 | true |
| Inverted | 131 | 1 1 0 1 | 2 4 5 7 | 5 5 2 2 | 1 1 0 1 | 1 1 1 1 | 1 1 0 1 | true |
| Inverted | 173 | 2 1 0 1 | 0 3 3 6 | 6 4 3 2 | 2 1 0 1 | 0 2 0 1 | 1 2 1 1 | true |
| Inverted | 211 | 2 1 0 1 | 1 5 5 6 | 8 2 3 1 | 2 1 0 1 | 1 1 0 1 | 1 0 2 1 | true |
| Inverted | 251 | 1 1 1 0 | 1 3 7 6 | 7 4 1 3 | 1 1 1 0 | 1 0 1 1 | 2 1 0 1 | true |
| Bimodal | 7 | 2 0 1 1 | 1 1 0 2 | 0 1 1 1 | 2 0 1 1 | 1 1 0 2 | 0 1 2 2 | false |
| Bimodal | 19 | 1 1 1 1 | 1 1 1 2 | 2 1 1 1 | 1 1 1 1 | 1 1 0 2 | 1 0 1 1 | false |
| Bimodal | 43 | 1 1 1 0 | 2 0 2 1 | 2 0 1 1 | 1 1 1 0 | 1 0 1 1 | 1 1 1 1 | true |
| Bimodal | 91 | 1 1 1 0 | 1 1 1 1 | 1 1 2 1 | 1 1 1 0 | 1 0 2 1 | 1 1 2 1 | false |
| Bimodal | 131 | 1 1 0 1 | 1 1 1 1 | 1 1 2 1 | 1 1 0 1 | 1 1 1 1 | 0 1 1 0 | false |
| Bimodal | 173 | 2 1 0 1 | 0 2 0 2 | 0 2 1 2 | 2 1 0 1 | 0 2 0 2 | 0 1 2 2 | false |
| Bimodal | 211 | 2 1 0 1 | 1 2 1 1 | 4 1 2 1 | 2 1 0 1 | 1 1 0 1 | 1 0 1 1 | false |
| Bimodal | 251 | 1 1 1 0 | 1 1 3 1 | 1 1 0 1 | 1 1 1 0 | 1 0 2 0 | 2 0 1 1 | true |

## AffordanceDiscovery strategy mix

Share of decisions per experiment in each contingent-condition phase, in percent.

| paradigm | phase | hand | jump | thrust | turn | strafe | look | voice | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Standard | baseline | 3 | 34 | 31 | 7 | 7 | 7 | 7 | 0 |
| Standard | connected | 18 | 31 | 27 | 5 | 5 | 5 | 6 | 0 |
| Standard | early extinction | 20 | 33 | 22 | 5 | 5 | 5 | 5 | 0 |
| Standard | late extinction | 10 | 32 | 29 | 6 | 6 | 7 | 7 | 0 |
| Inverted | baseline | 3 | 34 | 31 | 7 | 7 | 7 | 7 | 0 |
| Inverted | connected | 18 | 31 | 27 | 5 | 5 | 5 | 6 | 0 |
| Inverted | early extinction | 20 | 33 | 22 | 5 | 5 | 5 | 5 | 0 |
| Inverted | late extinction | 10 | 32 | 29 | 6 | 6 | 7 | 7 | 0 |
| Bimodal | baseline | 3 | 34 | 31 | 7 | 7 | 7 | 7 | 0 |
| Bimodal | connected | 4 | 33 | 33 | 6 | 7 | 7 | 6 | 0 |
| Bimodal | early extinction | 4 | 34 | 33 | 6 | 7 | 5 | 7 | 0 |
| Bimodal | late extinction | 4 | 33 | 33 | 7 | 7 | 6 | 6 | 0 |
