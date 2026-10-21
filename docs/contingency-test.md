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
| Standard | AffordanceDiscovery | Contingent | 5.8 | 17.9 | 14.0 | 7.8 | 138 | 0 → 95 → 60 → 4 |
| Standard | AffordanceDiscovery | Yoked | 5.8 | 7.0 | 7.8 | 7.8 | 138 | 0 → 24 → 32 → 4 |
| Inverted | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Inverted | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Inverted | AffordanceDiscovery | Contingent | 5.8 | 17.9 | 14.0 | 7.8 | 138 | 0 → 96 → 61 → 5 |
| Inverted | AffordanceDiscovery | Yoked | 5.8 | 7.0 | 7.8 | 7.8 | 138 | 0 → 25 → 33 → 5 |
| Bimodal | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Bimodal | AffordanceDiscovery | Contingent | 5.8 | 19.9 | 14.8 | 4.6 | 153 | 0 → 13 → 10 → 1 |
| Bimodal | AffordanceDiscovery | Yoked | 5.8 | 7.8 | 8.5 | 5.4 | 153 | 0 → 4 → 6 → 1 |
| Seen | PredictiveBaseline | Contingent | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | PredictiveBaseline | Yoked | 0.0 | 0.0 | 0.0 | 0.0 | 0 | 0 → 0 → 0 → 0 |
| Seen | Random | Contingent | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | Random | Yoked | 7.6 | 8.5 | 9.6 | 8.5 | 66 | 0 → 0 → 0 → 0 |
| Seen | AffordanceDiscovery | Contingent | 6.2 | 12.5 | 10.1 | 6.2 | 96 | 8 → 13 → 9 → 0 |
| Seen | AffordanceDiscovery | Yoked | 6.2 | 6.6 | 6.2 | 5.4 | 96 | 8 → 4 → 4 → 0 |

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

## Delayed consequences

The standard paradigm with contact arriving `delay` decisions later than the next observation. Hand pulses per 100 decisions; anticipation is the connected-phase mean.

| delay | controller | baseline | connected | early extinction | late extinction | yoked | anticipation | seeds passing |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 0 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 0 | AffordanceDiscovery | 5.8 | 17.9 | 14.0 | 7.8 | 7.0 | 95 | 8/8 |
| 1 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 1 | AffordanceDiscovery | 5.8 | 18.3 | 14.0 | 7.0 | 7.8 | 105 | 8/8 |
| 2 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 2 | AffordanceDiscovery | 5.8 | 18.7 | 14.8 | 7.0 | 7.0 | 100 | 8/8 |
| 4 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 4 | AffordanceDiscovery | 5.8 | 7.8 | 6.2 | 7.8 | 8.9 | 15 | 2/8 |
| 8 | Random | 7.6 | 8.5 | 9.6 | 8.5 | 8.5 | 0 | 0/8 |
| 8 | AffordanceDiscovery | 5.8 | 6.6 | 6.2 | 6.2 | 7.0 | 7 | 0/8 |

## AffordanceDiscovery learning curves

Hand pulses per 24 decisions for each seed, phases separated by `|`.

| paradigm | seed | contingent | yoked | passes |
|---|---:|---|---|---|
| Standard | 7 | 3 0 0 3 | 3 3 3 6 | 3 3 3 3 | 3 0 0 3 | 3 0 0 3 | 3 3 0 3 | true |
| Standard | 19 | 3 0 3 0 | 3 3 6 3 | 6 3 0 3 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Standard | 43 | 0 3 0 0 | 9 3 6 6 | 3 3 3 3 | 0 3 0 0 | 6 3 3 0 | 3 0 3 3 | true |
| Standard | 91 | 0 3 0 3 | 0 3 6 3 | 3 6 3 0 | 0 3 0 3 | 0 3 3 0 | 0 3 3 0 | true |
| Standard | 131 | 3 0 3 0 | 6 6 3 6 | 3 3 0 3 | 3 0 3 0 | 3 3 0 0 | 0 3 0 3 | true |
| Standard | 173 | 3 0 3 0 | 3 3 6 6 | 3 3 3 0 | 3 0 3 0 | 3 0 0 3 | 0 3 0 3 | true |
| Standard | 211 | 3 0 0 3 | 0 3 3 6 | 3 3 3 0 | 3 0 0 3 | 0 3 0 3 | 3 3 0 3 | true |
| Standard | 251 | 3 0 3 0 | 6 3 6 6 | 0 6 0 3 | 3 0 3 0 | 3 0 3 0 | 3 0 3 3 | true |
| Inverted | 7 | 3 0 0 3 | 3 3 3 6 | 3 3 3 3 | 3 0 0 3 | 3 0 0 3 | 3 3 0 3 | true |
| Inverted | 19 | 3 0 3 0 | 3 3 6 3 | 6 3 0 3 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Inverted | 43 | 0 3 0 0 | 9 3 6 6 | 3 3 3 3 | 0 3 0 0 | 6 3 3 0 | 3 0 3 3 | true |
| Inverted | 91 | 0 3 0 3 | 0 3 6 3 | 3 6 3 0 | 0 3 0 3 | 0 3 3 0 | 0 3 3 0 | true |
| Inverted | 131 | 3 0 3 0 | 6 6 3 6 | 3 3 0 3 | 3 0 3 0 | 3 3 0 0 | 0 3 0 3 | true |
| Inverted | 173 | 3 0 3 0 | 3 3 6 6 | 3 3 3 0 | 3 0 3 0 | 3 0 0 3 | 0 3 0 3 | true |
| Inverted | 211 | 3 0 0 3 | 0 3 3 6 | 3 3 3 0 | 3 0 0 3 | 0 3 0 3 | 3 3 0 3 | true |
| Inverted | 251 | 3 0 3 0 | 6 3 6 6 | 0 6 0 3 | 3 0 3 0 | 3 0 3 0 | 3 0 3 3 | true |
| Bimodal | 7 | 3 0 0 3 | 3 3 6 3 | 3 3 3 3 | 3 0 0 3 | 3 0 0 3 | 0 3 0 3 | true |
| Bimodal | 19 | 3 0 3 0 | 3 6 9 3 | 6 0 3 0 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Bimodal | 43 | 0 3 0 0 | 12 3 6 6 | 3 3 3 0 | 0 3 0 0 | 6 3 3 6 | 6 0 3 0 | true |
| Bimodal | 91 | 0 3 0 3 | 0 3 6 3 | 6 3 3 0 | 0 3 0 3 | 0 3 0 3 | 0 3 0 3 | true |
| Bimodal | 131 | 3 0 3 0 | 3 3 6 3 | 3 3 0 0 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Bimodal | 173 | 3 0 3 0 | 3 3 9 6 | 6 3 0 0 | 3 0 3 0 | 3 0 0 3 | 3 3 0 3 | true |
| Bimodal | 211 | 3 0 0 3 | 0 3 3 9 | 3 3 3 0 | 3 0 0 3 | 0 3 0 3 | 3 0 3 0 | true |
| Bimodal | 251 | 3 0 3 0 | 6 6 6 9 | 3 6 0 0 | 3 0 3 0 | 3 0 3 0 | 3 3 0 0 | true |
| Seen | 7 | 3 0 3 0 | 3 3 0 6 | 3 0 3 3 | 3 0 3 0 | 3 3 0 3 | 0 3 0 3 | false |
| Seen | 19 | 3 0 3 0 | 3 0 6 3 | 3 3 0 3 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Seen | 43 | 0 3 0 0 | 6 3 3 9 | 3 3 0 3 | 0 3 0 0 | 3 0 3 3 | 0 3 0 3 | true |
| Seen | 91 | 0 3 0 3 | 0 3 6 0 | 3 3 0 3 | 0 3 0 3 | 0 3 3 0 | 3 3 3 0 | true |
| Seen | 131 | 3 0 3 0 | 3 3 3 0 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | 3 0 3 0 | true |
| Seen | 173 | 3 0 3 0 | 3 0 3 6 | 3 3 0 3 | 3 0 3 0 | 3 0 0 3 | 0 3 0 0 | true |
| Seen | 211 | 3 0 3 3 | 0 3 3 3 | 3 0 3 0 | 3 0 3 3 | 0 3 0 0 | 3 0 3 0 | false |
| Seen | 251 | 3 0 0 3 | 0 3 3 6 | 3 3 0 0 | 3 0 0 3 | 0 3 0 3 | 0 0 3 0 | true |

## AffordanceDiscovery strategy mix

Share of decisions per experiment in each contingent-condition phase, in percent.

| paradigm | phase | hand | jump | thrust | turn | strafe | look | voice | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Standard | baseline | 5 | 16 | 22 | 12 | 14 | 14 | 13 | 0 |
| Standard | connected | 17 | 18 | 19 | 12 | 10 | 10 | 11 | 0 |
| Standard | early extinction | 14 | 14 | 18 | 11 | 12 | 14 | 14 | 0 |
| Standard | late extinction | 7 | 17 | 21 | 15 | 11 | 12 | 13 | 0 |
| Inverted | baseline | 5 | 16 | 22 | 12 | 14 | 14 | 13 | 0 |
| Inverted | connected | 17 | 18 | 19 | 12 | 10 | 10 | 11 | 0 |
| Inverted | early extinction | 14 | 14 | 18 | 11 | 12 | 14 | 14 | 0 |
| Inverted | late extinction | 7 | 17 | 21 | 15 | 11 | 12 | 13 | 0 |
| Bimodal | baseline | 5 | 16 | 22 | 12 | 14 | 14 | 13 | 0 |
| Bimodal | connected | 19 | 18 | 19 | 10 | 11 | 9 | 10 | 0 |
| Bimodal | early extinction | 14 | 16 | 14 | 13 | 13 | 12 | 14 | 0 |
| Bimodal | late extinction | 4 | 17 | 26 | 14 | 12 | 14 | 10 | 0 |
| Seen | baseline | 6 | 16 | 21 | 13 | 14 | 14 | 14 | 0 |
| Seen | connected | 12 | 18 | 22 | 11 | 12 | 11 | 10 | 0 |
| Seen | early extinction | 10 | 17 | 21 | 12 | 10 | 12 | 15 | 0 |
| Seen | late extinction | 6 | 18 | 23 | 12 | 14 | 12 | 11 | 0 |
