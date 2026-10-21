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
| Standard | AffordanceDiscovery | Contingent | 5.4 | 19.9 | 14.3 | 8.0 | 153 | 0 → 91 → 39 → 3 |
| Standard | AffordanceDiscovery | Yoked | 5.4 | 5.9 | 7.0 | 6.7 | 153 | 0 → 31 → 32 → 4 |
| Inverted | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | AffordanceDiscovery | Contingent | 5.4 | 19.9 | 14.3 | 8.0 | 153 | 0 → 92 → 40 → 4 |
| Inverted | AffordanceDiscovery | Yoked | 5.4 | 5.9 | 7.0 | 6.7 | 153 | 0 → 32 → 33 → 5 |
| Bimodal | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | AffordanceDiscovery | Contingent | 5.4 | 19.4 | 13.8 | 7.2 | 149 | 0 → 19 → 6 → 0 |
| Bimodal | AffordanceDiscovery | Yoked | 5.4 | 6.1 | 6.5 | 5.7 | 149 | 0 → 4 → 4 → 0 |
| Seen | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | AffordanceDiscovery | Contingent | 6.2 | 13.6 | 12.5 | 5.9 | 105 | 8 → 32 → 17 → 1 |
| Seen | AffordanceDiscovery | Yoked | 6.2 | 5.4 | 6.2 | 5.9 | 105 | 8 → 12 → 12 → 2 |

## Verdicts

A run shows contingency when connected-phase pulses are at least 1.5× both baseline and the yoked middle phase, and their rate falls below the connected rate by late extinction. Early extinction is not judged, so an extinction burst does not count against learning.

| paradigm | controller | pooled | seeds passing |
|---|---|---|---:|
| Standard | PredictiveBaseline | false | 0/8 |
| Standard | Random | false | 0/8 |
| Standard | AffordanceDiscovery | true | 7/8 |
| Inverted | PredictiveBaseline | false | 0/8 |
| Inverted | Random | false | 0/8 |
| Inverted | AffordanceDiscovery | true | 7/8 |
| Bimodal | PredictiveBaseline | false | 0/8 |
| Bimodal | Random | false | 0/8 |
| Bimodal | AffordanceDiscovery | true | 7/8 |
| Seen | PredictiveBaseline | false | 0/8 |
| Seen | Random | false | 0/8 |
| Seen | AffordanceDiscovery | true | 7/8 |

## Delayed consequences

The standard paradigm with contact arriving `delay` decisions later than the next observation. Hand pulses per 100 decisions; anticipation is the connected-phase mean.

| delay | controller | baseline | connected | early extinction | late extinction | yoked | anticipation | seeds passing |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 0 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 0 | AffordanceDiscovery | 5.4 | 19.9 | 14.3 | 8.0 | 5.9 | 91 | 7/8 |
| 1 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 1 | AffordanceDiscovery | 5.4 | 9.5 | 13.0 | 7.8 | 4.6 | 34 | 3/8 |
| 2 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 2 | AffordanceDiscovery | 5.4 | 5.2 | 8.0 | 6.7 | 4.6 | 13 | 1/8 |
| 4 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 4 | AffordanceDiscovery | 5.4 | 4.0 | 5.2 | 5.7 | 5.0 | 3 | 0/8 |
| 8 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 8 | AffordanceDiscovery | 5.4 | 4.0 | 3.9 | 6.7 | 4.6 | 3 | 0/8 |

## AffordanceDiscovery learning curves

Hand pulses per 24 decisions for each seed, phases separated by `|`.

| paradigm | seed | contingent | yoked | passes |
|---|---:|---|---|---|
| Standard | 7 | 2 1 1 0 | 9 5 5 6 | 4 2 3 2 | 2 1 1 0 | 4 2 1 2 | 1 2 2 2 | true |
| Standard | 19 | 1 1 1 2 | 4 5 5 6 | 7 1 1 2 | 1 1 1 2 | 1 1 1 1 | 1 2 2 1 | true |
| Standard | 43 | 1 2 2 0 | 5 4 6 4 | 4 2 3 2 | 1 2 2 0 | 1 1 2 1 | 1 2 1 1 | true |
| Standard | 91 | 1 2 1 1 | 1 5 6 5 | 4 3 2 2 | 1 2 1 1 | 2 3 1 1 | 2 2 2 2 | true |
| Standard | 131 | 1 2 0 2 | 7 4 6 7 | 3 3 1 1 | 1 2 0 2 | 1 1 3 2 | 1 3 2 1 | true |
| Standard | 173 | 2 1 1 5 | 1 4 4 4 | 4 4 2 2 | 2 1 1 5 | 1 1 1 1 | 1 3 2 2 | false |
| Standard | 211 | 1 2 0 2 | 0 8 4 5 | 5 2 3 2 | 1 2 0 2 | 1 1 0 1 | 2 1 1 2 | true |
| Standard | 251 | 2 0 1 1 | 3 4 5 6 | 5 2 1 2 | 2 0 1 1 | 1 0 2 4 | 1 2 1 2 | true |
| Inverted | 7 | 2 1 1 0 | 9 5 5 6 | 4 2 3 2 | 2 1 1 0 | 4 2 1 2 | 1 2 2 2 | true |
| Inverted | 19 | 1 1 1 2 | 4 5 5 6 | 7 1 1 2 | 1 1 1 2 | 1 1 1 1 | 1 2 2 1 | true |
| Inverted | 43 | 1 2 2 0 | 5 4 6 4 | 4 2 3 2 | 1 2 2 0 | 1 1 2 1 | 1 2 1 1 | true |
| Inverted | 91 | 1 2 1 1 | 1 5 6 5 | 4 3 2 2 | 1 2 1 1 | 2 3 1 1 | 2 2 2 2 | true |
| Inverted | 131 | 1 2 0 2 | 7 4 6 7 | 3 3 1 1 | 1 2 0 2 | 1 1 3 2 | 1 3 2 1 | true |
| Inverted | 173 | 2 1 1 5 | 1 4 4 4 | 4 4 2 2 | 2 1 1 5 | 1 1 1 1 | 1 3 2 2 | false |
| Inverted | 211 | 1 2 0 2 | 0 8 4 5 | 5 2 3 2 | 1 2 0 2 | 1 1 0 1 | 2 1 1 2 | true |
| Inverted | 251 | 2 0 1 1 | 3 4 5 6 | 5 2 1 2 | 2 0 1 1 | 1 0 2 4 | 1 2 1 2 | true |
| Bimodal | 7 | 2 1 1 0 | 9 7 3 8 | 4 3 2 2 | 2 1 1 0 | 2 2 1 2 | 3 1 2 2 | true |
| Bimodal | 19 | 1 1 1 2 | 2 4 7 5 | 6 1 1 3 | 1 1 1 2 | 1 1 1 1 | 2 1 2 1 | true |
| Bimodal | 43 | 1 2 2 0 | 3 3 6 4 | 6 2 2 1 | 1 2 2 0 | 1 1 1 1 | 1 1 1 1 | true |
| Bimodal | 91 | 1 2 1 1 | 1 5 6 6 | 4 3 1 1 | 1 2 1 1 | 1 3 3 1 | 2 1 2 1 | true |
| Bimodal | 131 | 1 2 0 2 | 4 4 6 8 | 4 3 2 1 | 1 2 0 2 | 1 2 1 2 | 1 2 1 1 | true |
| Bimodal | 173 | 2 1 1 5 | 1 3 3 5 | 4 3 3 2 | 2 1 1 5 | 1 1 2 1 | 2 2 1 2 | false |
| Bimodal | 211 | 1 2 0 2 | 0 7 4 5 | 4 1 2 2 | 1 2 0 2 | 0 5 1 1 | 2 1 2 1 | true |
| Bimodal | 251 | 2 0 1 1 | 2 4 8 6 | 4 1 2 1 | 2 0 1 1 | 2 1 2 1 | 1 2 1 1 | true |
| Seen | 7 | 2 0 2 1 | 3 3 4 5 | 3 1 2 1 | 2 0 2 1 | 1 1 1 1 | 1 2 2 2 | true |
| Seen | 19 | 1 1 3 2 | 2 2 7 4 | 5 1 1 1 | 1 1 3 2 | 2 1 3 1 | 2 1 2 1 | true |
| Seen | 43 | 1 2 1 1 | 2 2 6 5 | 4 2 2 1 | 1 2 1 1 | 1 1 2 1 | 2 1 2 1 | true |
| Seen | 91 | 1 3 2 1 | 1 3 2 3 | 5 3 1 1 | 1 3 2 1 | 1 2 1 1 | 1 2 1 1 | false |
| Seen | 131 | 1 2 1 1 | 3 2 4 5 | 3 3 2 1 | 1 2 1 1 | 3 1 1 1 | 1 1 1 2 | true |
| Seen | 173 | 2 1 1 3 | 2 4 2 4 | 2 4 2 2 | 2 1 1 3 | 1 1 1 1 | 1 2 2 1 | true |
| Seen | 211 | 1 1 1 3 | 3 6 3 4 | 3 2 2 1 | 1 1 1 3 | 2 2 1 0 | 1 2 2 1 | true |
| Seen | 251 | 1 1 3 1 | 1 2 3 3 | 5 2 1 2 | 1 1 3 1 | 1 1 3 1 | 3 1 0 2 | true |

## AffordanceDiscovery strategy mix

Share of decisions per experiment in each contingent-condition phase, in percent.

| paradigm | phase | hand | jump | thrust | turn | strafe | look | voice | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Standard | baseline | 5 | 22 | 25 | 12 | 10 | 11 | 11 | 0 |
| Standard | connected | 19 | 21 | 24 | 8 | 7 | 9 | 9 | 0 |
| Standard | early extinction | 14 | 14 | 22 | 11 | 12 | 11 | 12 | 0 |
| Standard | late extinction | 8 | 18 | 21 | 13 | 13 | 11 | 12 | 0 |
| Inverted | baseline | 5 | 22 | 25 | 12 | 10 | 11 | 11 | 0 |
| Inverted | connected | 19 | 21 | 24 | 8 | 7 | 9 | 9 | 0 |
| Inverted | early extinction | 14 | 14 | 22 | 11 | 12 | 11 | 12 | 0 |
| Inverted | late extinction | 8 | 18 | 21 | 13 | 13 | 11 | 12 | 0 |
| Bimodal | baseline | 5 | 22 | 25 | 12 | 10 | 11 | 11 | 0 |
| Bimodal | connected | 19 | 20 | 24 | 8 | 8 | 9 | 8 | 0 |
| Bimodal | early extinction | 13 | 19 | 22 | 12 | 10 | 9 | 11 | 0 |
| Bimodal | late extinction | 7 | 20 | 25 | 12 | 11 | 11 | 11 | 0 |
| Seen | baseline | 6 | 21 | 26 | 12 | 11 | 11 | 11 | 0 |
| Seen | connected | 13 | 20 | 25 | 10 | 10 | 10 | 10 | 0 |
| Seen | early extinction | 12 | 17 | 23 | 12 | 12 | 10 | 11 | 0 |
| Seen | late extinction | 5 | 20 | 24 | 12 | 11 | 12 | 12 | 0 |
