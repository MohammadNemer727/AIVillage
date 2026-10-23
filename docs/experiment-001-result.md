# Experiment 001: autonomous generational turnover

**Result date:** 2026-10-22

**Evidence:** `integrations/luanti/experiment-001-runs/2026-10-22-long-matched-2013Z-rerun2/`

**Status:** Passed the narrow identity-and-lifecycle objective.

## Result

Two natural, four-slot VoxeLibre cohorts ran from founders to extinction. Mutation-enabled
produced 7 births and mutation-disabled produced 9; both reached generation 2 (founders are
generation 0) and then naturally reached zero live individuals. This demonstrates autonomous
generational turnover and the implemented heredity path. It does **not** demonstrate a mutation
advantage, adaptation, improved fitness, learning, or intelligence. In this single matched pair,
the disabled cohort had more births, so no comparative effect should be inferred.

| Outcome | Mutation enabled | Mutation disabled |
| --- | ---: | ---: |
| Elapsed wall time | 742 s (12 min 22 s) | 782 s (13 min 2 s) |
| Controller reproduction attempts | 10,317 | 10,489 |
| Births | 7 | 9 |
| Deaths, including 4 founders | 11 | 13 |
| Maximum generation | 2 | 2 |
| Blocked: no free slot | 9,506 | 9,563 |
| Blocked: biological eligibility | 804 | 917 |
| Blocked: infrastructure | 0 | 0 |
| Final live individuals | 0 | 0 |
| Stop reason | `natural_extinction` | `natural_extinction` |

Capacity was four, so the high `no_free_slot` count is expected while all bodies were occupied;
it is reported rather than treated as failed fitness. Biological blocks mean the durable
maturity/reserve/cost checks rejected the request.

## Addendum (2026-10-23): what ended both cohorts

A later reading of the retained server logs shows that "natural extinction" was the first
VoxeLibre night. In both cohorts every founder was killed by a zombie (`mobs_mc:zombie`, 3 damage
per hit) about 11.4 minutes after the server started, which matches nightfall in VoxeLibre's
20-minute day. Each newborn reused a dead body's slot, so it appeared among the same zombies and
died within 36–156 decisions (under 40 seconds) even though it was fully fed. Lineage
reconstruction (`sim-luanti lineage`) also shows that no birth changed an expressed developmental
parameter, and founder ordinal 1 parented six of the seven mutation-enabled births. None of this
changes the narrow turnover result above, but survival in these cohorts measured how long a body
lasted against the first night, not what the founders did by day.

## Configuration and provenance

Both reports record:

- Git revision `97446703179fc1011ed03590286a07af3d54d694`
- source-tree content SHA-256
  `cefa552eba480367e9a1a0fffccb9ced8f5bacd9626eab702fc1526ad2f2a91c`
- seed 1, predictive `baseline` mode, 4 founders, capacity 4
- maturity 100 decisions, minimum reserve 160, reproduction cost 80
- birth target 100, deadline 86,400 seconds, and `forced_signal: false`
- canonical recovery disabled and excluded

The enabled provenance was created at `2026-10-22T20:12:03Z`; the disabled provenance at
`2026-10-22T20:24:45Z`. The cohorts used the same revision, source hash, seed, world setup, and
eligibility settings, differing only in mutation configuration. This is a matched live pair, not
a bit-exact game-physics replay or a replicated statistical comparison.

An immediately preceding attempted rerun
(`2026-10-22-long-matched-2012Z-rerun/enabled`) lost a launch/instrumentation race for the shared
bridge address and recorded `Address already in use`; it produced no result report and is excluded.
The `rerun2` evidence is the clean correction: cohorts were run sequentially, each bridge reached
natural extinction, and each wrote its own terminal report. Earlier incomplete runs are likewise
not result evidence.

## Audit invariants

Checks over each terminal report passed:

- attempts equal births plus blocked attempts;
- blocked reason totals equal the report summary, with zero infrastructure blocks;
- deaths equal founders plus births, and every individual was dead at termination;
- newborn IDs are unique, contiguous, and never reused;
- births include generation-1 and generation-2 descendants;
- no forced reproductive signal or canonical extinction recovery was used.

The terminal event streams contain 7 births/11 deaths for enabled and 9 births/13 deaths for
disabled. Their final manifests contain no live individuals. Together with the audited parent,
generation, body-slot, cost, and event IDs on every birth, this is the retained evidence for
turnover and heredity plumbing—not for beneficial mutation or evolved capability.

## Evidence and checksums

Paths below are relative to the evidence directory named above. SHA-256 identifies the exact
retained files used for this result.

| File | Mutation enabled | Mutation disabled |
| --- | --- | --- |
| `experiment-001-report.json` | `0fe52dd21c82fc36f964d96fea108f6a289ebe897cda4eedd5673a319d09b9e7` | `d7a87d9a24ac1c569bc17dc7f4033829c1428eeeca4540b82ff0769a013054d3` |
| `run-provenance.json` | `b6c9c6c964d5856a3100b3a5467cd45db3221632fb17dd2fa4e990ebc9cca0fb` | `1382af09958a3884fe57e2b6137d023118d05306fe45a22acf54922c193e551c` |
| `bridge-trace.ndjson` | `03f02789f6b99e21e801c8795e30347cda88b2a0962420111b141ae7a0478720` | `6fe9ebe30422cdc29053284dca7a2069834b47e0a012f3966026e90fafee62c3` |
| `population/population.json` | `9a2c08f143a0f70f0d06cb32df01d402b1db96b79c7300bda20e8e67eac1964d` | `5012151b0671844cffac334a9d9457faa52074f7f9c4894d54d6e0c326d43c8f` |

The full runtime worlds and traces are intentionally ignored by Git. They are large, mutable,
generated artifacts (including world databases, client state, logs, caches, and local
authentication material), so committing them would make source history noisy and could retain
machine-local or sensitive data. The publishable record is this compact result plus immutable
source/configuration identity and checksums; raw evidence may be retained or archived separately.

## Reproduction

From the repository root, use fresh world directories and run the cohorts sequentially because
the launcher uses shared default service ports:

```sh
integrations/luanti/experiment-001.sh enabled /tmp/experiment-001-enabled
integrations/luanti/experiment-001.sh disabled /tmp/experiment-001-disabled
```

`experiment-001.sh` records provenance before launch and writes the report, trace, and population
state into each world. Its defaults reproduce the configuration above. Exact live counts and
durations need not repeat because Luanti physics and scheduling are not promised bit-exact;
exactness claims apply to Rust replay over a retained trace.
