# 8C: spatial memory and navigation

**Result date:** 2026-10-23
**Protocol:** [`spatial-8c-preregistration.json`](spatial-8c-preregistration.json), frozen at commit
`e6ee9a8` before any of its 20 seeds ran.
**Machine report:** [`spatial-8c-result.json`](spatial-8c-result.json), sha256
`07934871c2b4ced9631218124d7253faff6f4de0e84e573093207b1081a586c7`. Raw worlds and traces stay out
of git.

## Decision

**Rejected under the frozen rules.** Two preregistered criteria failed. The main quantitative
effect was large, and this note reports both facts without softening the rejection: the rules were
fixed in advance and were not changed after seeing the outcome.

| Check | Result |
| --- | --- |
| Validity: 52 arms complete, all traces replay exactly, median dead-reckoning error ≤ 12 blocks | passed (median 7.9 blocks) |
| Primary (S against B): fed decisions +10 percent of 7200, relief entries +6, ≥ 9 of 12 pairs better with ≤ 1 worse, **harm entries of S not above B**, steering active | **failed on harm entries only** |
| Sham (S against X): fed decisions +5 percent, ≥ 9 of 12 pairs better with **≤ 1 worse** | **failed on pairs worse (2)** |

## Primary: 12 open arenas, 600 decisions each

| Arm | Fed decisions | Relief entries | Harm entries |
| --- | ---: | ---: | ---: |
| B no spatial memory | 2848 | 12 | 0 |
| S spatial memory | **3890** | **24** | 2 |
| X sham (wrong map) | 3038 | 14 | 0 |

S was fed 1042 decisions more than B (threshold 720; 37 percent more), returned to the patch twice
as often (24 entries against 12), was better than B in 11 of 12 pairs and worse in none. Per-seed
change in fed decisions, S minus B: +49, +64, +170, +180, +56, +6, +40, +129, 0, +139, +156, +53.

The primary rule failed on a single clause. The control never entered the harm patch in any seed,
while S entered it once in seed 9213 and once in seed 9231. Heading purposefully back toward the
relief patch carried the body into harm twice before it had learned to avoid it. Counted exactly as
preregistered, 2 is above 0, so the criterion fails.

## Sham: is the map authentic?

The sham stored every remembered place mirrored through the birth point and shifted 24 blocks, with
everything else identical. S beat X by 852 fed decisions (threshold 360), and was better in 9 pairs.
Two pairs favoured X: seed 9201 by 122 and seed 9239 by 6. In those seeds the wrong map's search
happened to cross the real patch (X was already above B by 171 and 59 decisions there). Across the
other ten seeds X is level with B, so the sham behaves as a control, but two pairs worse is above
the preregistered allowance of one.

## Wall variant (report only, 8 seeds)

| Arm | Fed decisions | Relief entries |
| --- | ---: | ---: |
| B | 1912 | 8 |
| S | 2980 | 18 |

S was no worse than B in any of the eight pairs (one tie at 239 each, the rest +39 to +267). The
detour reflex (sidestep after pushing against something) let the body get around an 11-block wall
that blocks the direct way back. This variant does not decide anything.

## Navigation quality and bounds

- Dead reckoning drifted about 8 blocks at the median (3.0 to 13.1 by seed, over 600 decisions),
  comparable to the 6-block relief patch. The body therefore usually arrives near, not at, the place
  and relies on the 90-decision local search to find the patch.
- Spatial memory never held more than 3 places (limit 16).
- Steering was active for 3035 decisions across the 12 S arms.

## What this does and does not show

- A body that has felt relief once can, using only its felt heading, its own motor commands and
  clock, and touch, get back to the place far more reliably than a body without the memory, and the
  benefit is mostly absent when the remembered map is wrong.
- It does not show safe navigation: the memory-guided body took more harm than the control, and a
  stricter harm-avoidance or a harm-aware route would be needed before calling it a navigation
  improvement.
- The evidence is for this fixture: a flat open arena, a large patch, a fixed exploratory gait
  standing in for evolved exploration, and the episodic policy switched off. It is not evidence for
  unfamiliar terrain, caves, or obstacles beyond one wall.
- The mechanism was iterated on development pilots (nine sets of consumed seeds, listed in the
  preregistration) before the protocol was frozen. The final design was not tuned on the 20 seeds
  above.
- Under the plan's rule, communication and culture remain blocked, but for reasons that predate this
  result (see `communication-culture-readiness.md`).
