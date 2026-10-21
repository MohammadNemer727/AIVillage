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
| Standard | AffordanceDiscovery | Contingent | 4.8 | 19.4 | 21.8 | 11.1 | 149 | 0 → 108 → 58 → 7 |
| Standard | AffordanceDiscovery | Yoked | 4.8 | 4.8 | 5.7 | 5.7 | 149 | 0 → 26 → 34 → 7 |
| Inverted | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | AffordanceDiscovery | Contingent | 4.8 | 19.4 | 21.8 | 11.1 | 149 | 0 → 108 → 59 → 8 |
| Inverted | AffordanceDiscovery | Yoked | 4.8 | 4.8 | 5.7 | 5.7 | 149 | 0 → 27 → 35 → 8 |
| Bimodal | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | AffordanceDiscovery | Contingent | 4.8 | 24.6 | 27.0 | 8.3 | 189 | 0 → 20 → 11 → 1 |
| Bimodal | AffordanceDiscovery | Yoked | 4.8 | 4.1 | 6.7 | 5.4 | 189 | 0 → 5 → 9 → 1 |
| Seen | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | AffordanceDiscovery | Contingent | 4.8 | 10.1 | 19.7 | 7.2 | 78 | 12 → 25 → 17 → 2 |
| Seen | AffordanceDiscovery | Yoked | 4.8 | 4.1 | 4.4 | 5.4 | 78 | 12 → 9 → 11 → 2 |

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
| Bimodal | AffordanceDiscovery | true | 8/8 |
| Seen | PredictiveBaseline | false | 0/8 |
| Seen | Random | false | 0/8 |
| Seen | AffordanceDiscovery | true | 6/8 |

## AffordanceDiscovery learning curves

Hand pulses per 24 decisions for each seed, phases separated by `|`.

| paradigm | seed | contingent | yoked | passes |
|---|---:|---|---|---|
| Standard | 7 | 2 0 1 1 | 3 5 4 9 | 7 3 5 4 | 2 0 1 1 | 1 1 0 2 | 1 3 1 2 | true |
| Standard | 19 | 1 1 0 2 | 4 5 7 5 | 7 2 2 3 | 1 1 0 2 | 2 0 1 1 | 2 1 2 1 | true |
| Standard | 43 | 1 1 1 2 | 5 4 7 6 | 9 3 2 1 | 1 1 1 2 | 1 1 2 0 | 1 1 1 1 | true |
| Standard | 91 | 1 1 1 2 | 0 7 5 5 | 9 2 2 1 | 1 1 1 2 | 1 4 2 1 | 1 1 2 0 | true |
| Standard | 131 | 1 1 1 1 | 6 4 6 5 | 8 2 3 2 | 1 1 1 1 | 1 1 1 1 | 0 2 1 1 | true |
| Standard | 173 | 2 0 1 2 | 3 5 5 5 | 9 3 2 3 | 2 0 1 2 | 1 1 1 1 | 1 2 2 1 | true |
| Standard | 211 | 2 1 0 3 | 0 5 4 4 | 9 2 4 2 | 2 1 0 3 | 0 3 1 0 | 1 2 2 2 | true |
| Standard | 251 | 1 0 2 1 | 1 5 5 5 | 6 3 2 5 | 1 0 2 1 | 1 1 2 1 | 1 2 1 2 | true |
| Inverted | 7 | 2 0 1 1 | 3 5 4 9 | 7 3 5 4 | 2 0 1 1 | 1 1 0 2 | 1 3 1 2 | true |
| Inverted | 19 | 1 1 0 2 | 4 5 7 5 | 7 2 2 3 | 1 1 0 2 | 2 0 1 1 | 2 1 2 1 | true |
| Inverted | 43 | 1 1 1 2 | 5 4 7 6 | 9 3 2 1 | 1 1 1 2 | 1 1 2 0 | 1 1 1 1 | true |
| Inverted | 91 | 1 1 1 2 | 0 7 5 5 | 9 2 2 1 | 1 1 1 2 | 1 4 2 1 | 1 1 2 0 | true |
| Inverted | 131 | 1 1 1 1 | 6 4 6 5 | 8 2 3 2 | 1 1 1 1 | 1 1 1 1 | 0 2 1 1 | true |
| Inverted | 173 | 2 0 1 2 | 3 5 5 5 | 9 3 2 3 | 2 0 1 2 | 1 1 1 1 | 1 2 2 1 | true |
| Inverted | 211 | 2 1 0 3 | 0 5 4 4 | 9 2 4 2 | 2 1 0 3 | 0 3 1 0 | 1 2 2 2 | true |
| Inverted | 251 | 1 0 2 1 | 1 5 5 5 | 6 3 2 5 | 1 0 2 1 | 1 1 2 1 | 1 2 1 2 | true |
| Bimodal | 7 | 2 0 1 1 | 3 7 7 10 | 9 6 2 4 | 2 0 1 1 | 1 1 1 1 | 1 2 1 2 | true |
| Bimodal | 19 | 1 1 0 2 | 3 4 11 8 | 11 3 2 2 | 1 1 0 2 | 1 1 0 1 | 2 1 1 1 | true |
| Bimodal | 43 | 1 1 1 2 | 4 5 9 8 | 8 2 2 1 | 1 1 1 2 | 1 1 1 1 | 2 1 1 1 | true |
| Bimodal | 91 | 1 1 1 2 | 0 7 6 7 | 11 4 2 1 | 1 1 1 2 | 0 1 2 0 | 1 2 2 0 | true |
| Bimodal | 131 | 1 1 1 1 | 5 6 6 8 | 8 3 1 1 | 1 1 1 1 | 1 1 1 1 | 1 1 1 1 | true |
| Bimodal | 173 | 2 0 1 2 | 3 6 7 8 | 9 6 2 3 | 2 0 1 2 | 1 1 0 2 | 3 4 3 2 | true |
| Bimodal | 211 | 2 1 0 3 | 0 4 4 6 | 11 1 3 2 | 2 1 0 3 | 0 3 2 0 | 1 1 3 0 | true |
| Bimodal | 251 | 1 0 2 1 | 1 7 9 10 | 9 3 1 3 | 1 0 2 1 | 1 1 2 1 | 1 2 1 1 | true |
| Seen | 7 | 2 0 1 1 | 3 2 2 3 | 4 3 3 4 | 2 0 1 1 | 1 1 0 1 | 1 2 1 3 | false |
| Seen | 19 | 1 1 0 2 | 2 3 3 3 | 10 3 2 2 | 1 1 0 2 | 4 2 0 1 | 2 1 2 1 | true |
| Seen | 43 | 1 1 1 1 | 3 1 4 4 | 7 3 1 1 | 1 1 1 1 | 1 0 1 1 | 1 1 1 1 | true |
| Seen | 91 | 1 1 2 1 | 1 2 2 3 | 4 4 2 1 | 1 1 2 1 | 1 1 1 0 | 1 1 2 0 | true |
| Seen | 131 | 1 1 1 1 | 4 2 2 5 | 4 4 1 1 | 1 1 1 1 | 1 1 0 1 | 1 1 1 1 | true |
| Seen | 173 | 2 0 3 3 | 1 2 1 3 | 5 5 3 1 | 2 0 3 3 | 1 1 1 1 | 0 2 2 1 | false |
| Seen | 211 | 1 1 0 2 | 1 3 2 2 | 7 1 3 0 | 1 1 0 2 | 1 2 1 1 | 1 0 2 1 | true |
| Seen | 251 | 1 0 2 1 | 1 2 4 2 | 9 3 1 2 | 1 0 2 1 | 1 0 2 1 | 0 2 0 2 | true |

## AffordanceDiscovery strategy mix

Share of decisions per experiment in each contingent-condition phase, in percent.

| paradigm | phase | hand | jump | thrust | turn | strafe | look | voice | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Standard | baseline | 4 | 29 | 31 | 8 | 8 | 9 | 8 | 0 |
| Standard | connected | 19 | 35 | 23 | 5 | 5 | 5 | 5 | 0 |
| Standard | early extinction | 21 | 15 | 17 | 11 | 10 | 11 | 11 | 0 |
| Standard | late extinction | 11 | 20 | 25 | 11 | 10 | 10 | 10 | 0 |
| Inverted | baseline | 4 | 29 | 31 | 8 | 8 | 9 | 8 | 0 |
| Inverted | connected | 19 | 35 | 23 | 5 | 5 | 5 | 5 | 0 |
| Inverted | early extinction | 21 | 15 | 17 | 11 | 10 | 11 | 11 | 0 |
| Inverted | late extinction | 11 | 20 | 25 | 11 | 10 | 10 | 10 | 0 |
| Bimodal | baseline | 4 | 29 | 31 | 8 | 8 | 9 | 8 | 0 |
| Bimodal | connected | 24 | 29 | 23 | 4 | 5 | 6 | 6 | 0 |
| Bimodal | early extinction | 27 | 19 | 17 | 9 | 7 | 8 | 9 | 0 |
| Bimodal | late extinction | 8 | 23 | 30 | 9 | 10 | 9 | 8 | 0 |
| Seen | baseline | 4 | 26 | 31 | 9 | 8 | 10 | 9 | 0 |
| Seen | connected | 10 | 26 | 30 | 8 | 8 | 8 | 7 | 0 |
| Seen | early extinction | 19 | 23 | 24 | 8 | 7 | 7 | 8 | 0 |
| Seen | late extinction | 7 | 24 | 31 | 9 | 9 | 8 | 9 | 0 |
