//! Experiment 004 matched one-locus-at-a-time probes.
//!
//! Every probe feeds the same scripted, action-independent observation stream to two genomes
//! that differ in exactly one locus (a low and a high setting, everything else at the midpoint)
//! and compares one pre-declared behavioural measure. Observations never depend on what the
//! body does, so a pair is matched exactly and any difference is that locus.
//!
//! The runtime under test is the real [`AgentRuntime`] decision loop (and, for the life-history
//! loci, the real [`PopulationCoordinator`] eligibility and birth path), not a re-implementation.

#![allow(
    clippy::cast_precision_loss,
    reason = "counts and ticks are far below 2^52, and the measures are compared, not decided on exactly"
)]

use serde::Serialize;
use sim_core::{
    DevelopmentalGenome, RawSensoryFrame,
    environment::{
        BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, Interoception,
        PrimitiveAction, TransportStatus,
    },
    phenotype::{ExpressionVersion, LOCUS_ROLES},
    runtime::{AgentRuntime, ExperimentMode},
};

use sim_format::PopulationCommandResult;

use crate::{
    ApplicationTrace, AppliedPose, BridgeRequest, LifecycleCommand, LifecycleConfig,
    LifecycleOperation, PROTOCOL_VERSION, PopulationCoordinator,
};

/// Decisions in every probe run.
pub const PROBE_DECISIONS: u64 = 600;
/// Locus value of the low arm of a matched pair.
pub const LOW: u8 = 8;
/// Locus value of the high arm of a matched pair.
pub const HIGH: u8 = 56;
/// Locus value of every other locus in a matched pair.
pub const MIDPOINT: u8 = 32;
/// Mutation probability carried by probe genomes; no probe mutates.
const PROBE_MUTATION_PPM: u16 = 12_000;
/// Felt fullness below which a decision counts as hungry (the drive's own threshold).
const HUNGRY_BELOW: u16 = sim_core::phenotype::HUNGER_DRIVE_BELOW;

/// Felt fullness at decision `tick`: fed for 300 decisions (long enough to reproduce), hungry
/// for about 60 near decision 450 as it falls, then fed again. Real founders sit near full for
/// long stretches, and reproductive reserve cannot exceed fullness.
fn fullness(tick: u64) -> u16 {
    if (300..450).contains(&tick) {
        255 - u16::try_from((tick - 300) * 255 / 150).unwrap_or(255)
    } else {
        255
    }
}

/// Pain at decision `tick`: a brief injury every 70 decisions.
fn pain(tick: u64) -> u16 {
    if tick % 70 < 3 { 90 } else { 0 }
}

/// The scripted observation for decision `tick`, independent of any action.
#[must_use]
pub fn observation(tick: u64) -> EnvironmentObservation {
    let mut senses = RawSensoryFrame::EMPTY;
    for (index, ray) in senses.retinal.iter_mut().enumerate() {
        let phase = (tick + index as u64 * 3) % 40;
        ray.brightness = u16::try_from(60 + phase * 4).unwrap_or(255);
        ray.motion = i16::try_from(phase).unwrap_or_default() - 20;
    }
    for (index, chemical) in senses.chemicals.iter_mut().enumerate() {
        *chemical = u16::try_from(30 + (tick + index as u64 * 7) % 60).unwrap_or(255);
    }
    EnvironmentObservation {
        sequence: tick,
        senses,
        body: BodyProprioception::default(),
        capabilities: EnvironmentCapabilities::default(),
        transport: TransportStatus::Healthy,
        retina: None,
        interoception: Some(Interoception {
            health: 220,
            fullness: fullness(tick),
            air: 255,
            pain: pain(tick),
        }),
        held_item: None,
    }
}

/// The base genome every matched pair is built on: the midpoint everywhere except the additive
/// drives (hunger, pain, tone), which are off, so that a quantity added to the action by one drive
/// does not mask what another locus changes. Each drive is probed on its own pair.
#[must_use]
pub fn matched_base() -> DevelopmentalGenome {
    [3, 4, 9].into_iter().fold(
        DevelopmentalGenome::uniform(MIDPOINT, PROBE_MUTATION_PPM),
        |base, locus| base.with_locus(locus, 0),
    )
}

/// A runtime built for `genome` under v2 expression in the evolving founders' baseline mode.
fn runtime(seed: u64, genome: &DevelopmentalGenome) -> AgentRuntime {
    let mut runtime = AgentRuntime::new_with_mode(
        seed,
        1,
        genome.default_brain_config(),
        ExperimentMode::PredictiveBaseline,
    );
    runtime.set_phenotype(genome.phenotype(ExpressionVersion::V2));
    runtime
}

/// Every action a genome chooses on the scripted stream.
///
/// # Panics
///
/// Panics if the runtime rejects the scripted, in-sequence observations.
#[must_use]
pub fn actions(seed: u64, genome: &DevelopmentalGenome) -> Vec<PrimitiveAction> {
    let mut body = runtime(seed, genome);
    (0..PROBE_DECISIONS)
        .map(|tick| {
            body.step(observation(tick))
                .expect("the scripted stream is in sequence")
                .action
        })
        .collect()
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0_u32), |(sum, count), value| (sum + value, count + 1));
    if count == 0 {
        0.0
    } else {
        sum / f64::from(count)
    }
}

fn request(tick: u64) -> BridgeRequest {
    BridgeRequest {
        protocol: PROTOCOL_VERSION,
        agent_id: 1,
        body_slot: None,
        observation: observation(tick),
        trace: ApplicationTrace {
            applied_server_step: tick,
            applied_steps: 1,
            accumulated_dtime_us: 100_000,
            applied_action: PrimitiveAction::default(),
            pose: AppliedPose {
                x_millinode: 0,
                y_millinode: 1_000,
                z_millinode: 0,
                yaw_milliradian: 0,
            },
        },
    }
}

/// Life history of a genome in the real lifecycle path: (tick of first birth, births).
///
/// The individual asks to reproduce on every decision; the coordinator decides, from the
/// individual's own expressed maturity, reserve threshold and birth cost.
fn life_history(seed: u64, genome: &DevelopmentalGenome) -> (f64, f64) {
    let mut population = PopulationCoordinator::new_with_genome(
        seed,
        1,
        genome.clone(),
        ExperimentMode::PredictiveBaseline,
    );
    population
        .set_capacity(256)
        .expect("one occupied slot fits");
    population
        .set_lifecycle_config(LifecycleConfig {
            maturity_decisions: 100,
            minimum_reproductive_reserve: 160,
            reproductive_cost: 80,
            mutation_enabled: false,
        })
        .expect("cost does not exceed the reserve");
    population.set_expression(ExpressionVersion::V2);
    let (mut first, mut births) = (PROBE_DECISIONS, 0_u32);
    for tick in 0..PROBE_DECISIONS {
        population.handle(request(tick)).expect("in sequence");
        let result = population
            .process_lifecycle(LifecycleCommand {
                event_id: format!("probe-{tick}"),
                operation: LifecycleOperation::Reproduce {
                    parent_id: 1,
                    body_slot: 0,
                    infrastructure_failures: 0,
                },
            })
            .expect("a live parent");
        if matches!(result, PopulationCommandResult::Born { .. }) {
            if births == 0 {
                first = tick;
            }
            births += 1;
        }
    }
    (first as f64, f64::from(births))
}

/// Which way the high arm is expected to move the measure, from what the parameter means.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Direction {
    /// The high arm's measure should exceed the low arm's.
    HighGreater,
    /// The high arm's measure should be below the low arm's.
    HighLess,
    /// No direction is predicted a priori; the effect must be consistent in sign across seeds.
    Either,
}

/// A locus's pre-declared measure.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Probe {
    /// Locus index.
    pub locus: usize,
    /// What is measured.
    pub measure: &'static str,
    /// The expected direction of high against low.
    pub expected: Direction,
}

/// The probes, one per locus, declared before any matched run.
pub const PROBES: [Probe; DevelopmentalGenome::LOCUS_COUNT] = [
    probe(
        0,
        "mean absolute thrust over all decisions",
        Direction::HighGreater,
    ),
    probe(
        1,
        "mean absolute turn over all decisions",
        Direction::HighGreater,
    ),
    probe(
        2,
        "fraction of fed, unhurt decisions with any thrust or turn carried out",
        Direction::HighLess,
    ),
    probe(
        3,
        "mean thrust over hungry decisions (felt fullness below 96)",
        Direction::HighGreater,
    ),
    probe(
        4,
        "mean thrust over decisions in which pain is felt",
        Direction::HighGreater,
    ),
    probe(
        5,
        "mean jaw amplitude over all decisions",
        Direction::HighGreater,
    ),
    probe(
        6,
        "fraction of decisions whose action differs between the arms",
        Direction::HighGreater,
    ),
    probe(
        7,
        "fraction of decisions whose action differs between the arms",
        Direction::HighGreater,
    ),
    probe(
        8,
        "fraction of decisions whose action differs between the arms",
        Direction::HighGreater,
    ),
    probe(
        9,
        "mean thrust over fed, unhurt decisions",
        Direction::HighGreater,
    ),
    probe(
        10,
        "fraction of decisions whose action differs between the arms",
        Direction::HighGreater,
    ),
    probe(11, "births in 600 decisions", Direction::HighGreater),
    probe(
        12,
        "decision of the first birth (600 if none)",
        Direction::HighGreater,
    ),
    probe(
        13,
        "fraction of decisions whose action differs between the arms",
        Direction::HighGreater,
    ),
    probe(14, "births in 600 decisions", Direction::HighLess),
    probe(15, "births in 600 decisions", Direction::HighLess),
];

/// Loci whose measure is how often the high arm's action differs from the low arm's: their
/// parameters reshape what the brain does rather than add a quantity to the action.
const DIFFERENCE_LOCI: [usize; 5] = [6, 7, 8, 10, 13];

const fn probe(locus: usize, measure: &'static str, expected: Direction) -> Probe {
    Probe {
        locus,
        measure,
        expected,
    }
}

/// The pre-declared measure of `locus` for one genome on one seed.
///
/// The loci in [`DIFFERENCE_LOCI`] have no scalar of their own; use [`pair`] for them.
#[must_use]
pub fn measure(locus: usize, seed: u64, genome: &DevelopmentalGenome) -> f64 {
    let fed_and_unhurt = |tick: &u64| fullness(*tick) >= HUNGRY_BELOW && pain(*tick) == 0;
    match locus {
        0 => mean(
            actions(seed, genome)
                .iter()
                .map(|action| f64::from(action.thrust.abs())),
        ),
        1 => mean(
            actions(seed, genome)
                .iter()
                .map(|action| f64::from(action.turn.abs())),
        ),
        2 => mean(
            actions(seed, genome)
                .iter()
                .zip(0..PROBE_DECISIONS)
                .filter(|(_, tick)| fed_and_unhurt(tick))
                .map(|(action, _)| f64::from(u8::from(action.thrust != 0 || action.turn != 0))),
        ),
        3 => mean(
            actions(seed, genome)
                .iter()
                .zip(0..PROBE_DECISIONS)
                .filter(|(_, tick)| fullness(*tick) < HUNGRY_BELOW)
                .map(|(action, _)| f64::from(action.thrust)),
        ),
        4 => mean(
            actions(seed, genome)
                .iter()
                .zip(0..PROBE_DECISIONS)
                .filter(|(_, tick)| pain(*tick) > 0)
                .map(|(action, _)| f64::from(action.thrust)),
        ),
        5 => mean(
            actions(seed, genome)
                .iter()
                .map(|action| f64::from(action.jaw)),
        ),
        9 => mean(
            actions(seed, genome)
                .iter()
                .zip(0..PROBE_DECISIONS)
                .filter(|(_, tick)| fed_and_unhurt(tick))
                .map(|(action, _)| f64::from(action.thrust)),
        ),
        11 | 14 | 15 => life_history(seed, genome).1,
        12 => life_history(seed, genome).0,
        _ => f64::NAN,
    }
}

/// The measure for the low and high arms of one matched pair on one seed.
///
/// For the [`DIFFERENCE_LOCI`] there is no scalar to compare, so the low arm is the reference (0)
/// and the high arm's measure is the fraction of decisions in which its action differs from the
/// low arm's.
#[must_use]
pub fn pair(locus: usize, seed: u64) -> (f64, f64) {
    let base = matched_base();
    let low = base.with_locus(locus, LOW);
    let high = base.with_locus(locus, HIGH);
    if DIFFERENCE_LOCI.contains(&locus) {
        let reference = actions(seed, &low);
        let chosen = actions(seed, &high);
        let differing = chosen
            .iter()
            .zip(&reference)
            .filter(|(left, right)| left != right)
            .count();
        return (0.0, differing as f64 / chosen.len() as f64);
    }
    (measure(locus, seed, &low), measure(locus, seed, &high))
}

/// Result of one locus's matched probe over all seeds.
#[derive(Clone, Debug, Serialize)]
pub struct LocusResult {
    /// Locus index.
    pub locus: usize,
    /// Parameter the locus is expressed as.
    pub parameter: &'static str,
    /// Measure used.
    pub measure: &'static str,
    /// Expected direction of high against low.
    pub expected: Direction,
    /// Low-arm measure per seed.
    pub low: Vec<f64>,
    /// High-arm measure per seed.
    pub high: Vec<f64>,
    /// Seeds in which the high arm moved the measure the expected way.
    pub seeds_in_expected_direction: usize,
    /// Whether the high arm raised or lowered the measure in the majority of seeds.
    pub observed_direction: Direction,
    /// Median over seeds of the relative effect, `|high - low| / max(|high|, |low|)`.
    pub median_relative_effect: f64,
    /// Whether the locus met the preregistered criterion.
    pub passes: bool,
}

/// Seeds in which the effect must go the expected way (of the seeds run).
pub const MIN_SEEDS_IN_DIRECTION: usize = 7;
/// Smallest median relative effect that counts as a measurable behavioural change.
pub const MIN_RELATIVE_EFFECT: f64 = 0.10;
/// For the difference loci: smallest fraction of decisions whose action differs between arms.
pub const MIN_ACTION_DIFFERENCE: f64 = 0.02;

/// Summarises one locus's matched pairs against its pre-declared criterion.
fn summarise(probe: &Probe, pairs: &[(f64, f64)]) -> LocusResult {
    let up = pairs.iter().filter(|(low, high)| high > low).count();
    let down = pairs.iter().filter(|(low, high)| high < low).count();
    let observed_direction = if up >= down {
        Direction::HighGreater
    } else {
        Direction::HighLess
    };
    // A predicted direction must hold in the seeds; with none predicted, the seeds must agree
    // with one another.
    let seeds_in_expected_direction = match probe.expected {
        Direction::HighGreater => up,
        Direction::HighLess => down,
        Direction::Either => up.max(down),
    };
    let mut effects: Vec<f64> = pairs
        .iter()
        .map(|(low, high)| {
            let scale = low.abs().max(high.abs());
            if scale == 0.0 {
                0.0
            } else {
                (high - low).abs() / scale
            }
        })
        .collect();
    effects.sort_by(f64::total_cmp);
    let mut median_relative_effect = effects[effects.len() / 2];
    let mut seeds_in_expected_direction = seeds_in_expected_direction;
    let mut meets_effect = median_relative_effect >= MIN_RELATIVE_EFFECT;
    if DIFFERENCE_LOCI.contains(&probe.locus) {
        // The measure is already the fraction of decisions that differ.
        let mut differences: Vec<f64> = pairs.iter().map(|pair| pair.1).collect();
        differences.sort_by(f64::total_cmp);
        median_relative_effect = differences[differences.len() / 2];
        seeds_in_expected_direction = differences
            .iter()
            .filter(|difference| **difference >= MIN_ACTION_DIFFERENCE)
            .count();
        meets_effect = median_relative_effect >= MIN_ACTION_DIFFERENCE;
    }
    LocusResult {
        locus: probe.locus,
        parameter: LOCUS_ROLES[probe.locus].parameter,
        measure: probe.measure,
        expected: probe.expected,
        observed_direction,
        low: pairs.iter().map(|pair| pair.0).collect(),
        high: pairs.iter().map(|pair| pair.1).collect(),
        seeds_in_expected_direction,
        median_relative_effect,
        passes: seeds_in_expected_direction >= MIN_SEEDS_IN_DIRECTION && meets_effect,
    }
}

/// Runs the matched probe for every locus over `seeds`.
#[must_use]
pub fn run_all(seeds: &[u64]) -> Vec<LocusResult> {
    PROBES
        .iter()
        .map(|probe| {
            let pairs: Vec<_> = seeds.iter().map(|seed| pair(probe.locus, *seed)).collect();
            summarise(probe, &pairs)
        })
        .collect()
}

/// Behavioural effect of one ordinary birth mutation: the largest step mutation makes (two) away
/// from the midpoint, in either direction.
#[derive(Clone, Debug, Serialize)]
pub struct MutationEffect {
    /// Locus index.
    pub locus: usize,
    /// Parameter the locus is expressed as.
    pub parameter: &'static str,
    /// Mutation step applied, +2 or -2.
    pub step: i8,
    /// Change in the expressed parameter, in its own units.
    pub parameter_change: i32,
    /// Fraction of seeds in which the pre-declared measure changed at all.
    pub fraction_of_seeds_changed: f64,
    /// Median over seeds of the relative change of the measure (or, for loci measured by how
    /// often the action differs, the fraction of decisions that differ).
    pub median_effect: f64,
}

/// Measures the effect of a ±2 mutation at every locus, over `seeds`.
#[must_use]
pub fn mutation_effects(seeds: &[u64]) -> Vec<MutationEffect> {
    let mut effects = Vec::new();
    for probe in &PROBES {
        // Mutate around this locus's own midpoint, with the other additive drives still off.
        let base = matched_base().with_locus(probe.locus, MIDPOINT);
        for step in [-2_i8, 2] {
            let mutant = base.with_locus(
                probe.locus,
                u8::try_from(i16::from(MIDPOINT) + i16::from(step)).unwrap_or(MIDPOINT),
            );
            let parameter_change = mutant.phenotype(ExpressionVersion::V2).parameters()
                [probe.locus]
                - base.phenotype(ExpressionVersion::V2).parameters()[probe.locus];
            let mut changed = 0_usize;
            let mut sizes: Vec<f64> = seeds
                .iter()
                .map(|seed| {
                    let (before, after) = if DIFFERENCE_LOCI.contains(&probe.locus) {
                        let reference = actions(*seed, &base);
                        let chosen = actions(*seed, &mutant);
                        let differing = chosen
                            .iter()
                            .zip(&reference)
                            .filter(|(left, right)| left != right)
                            .count();
                        (0.0, differing as f64 / chosen.len() as f64)
                    } else {
                        (
                            measure(probe.locus, *seed, &base),
                            measure(probe.locus, *seed, &mutant),
                        )
                    };
                    changed += usize::from((after - before).abs() > f64::EPSILON);
                    let scale = before.abs().max(after.abs());
                    if DIFFERENCE_LOCI.contains(&probe.locus) {
                        after
                    } else if scale == 0.0 {
                        0.0
                    } else {
                        (after - before).abs() / scale
                    }
                })
                .collect();
            sizes.sort_by(f64::total_cmp);
            effects.push(MutationEffect {
                locus: probe.locus,
                parameter: LOCUS_ROLES[probe.locus].parameter,
                step,
                parameter_change,
                fraction_of_seeds_changed: changed as f64 / seeds.len() as f64,
                median_effect: sizes[sizes.len() / 2],
            });
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripted_stream_is_action_independent_and_in_sequence() {
        assert_eq!(observation(10), observation(10));
        assert_eq!(observation(11).sequence, 11);
        assert!((0..PROBE_DECISIONS).any(|tick| pain(tick) > 0));
        assert!((0..PROBE_DECISIONS).any(|tick| fullness(tick) < HUNGRY_BELOW));
    }

    #[test]
    fn probes_are_declared_for_every_locus_in_order() {
        for (index, probe) in PROBES.iter().enumerate() {
            assert_eq!(probe.locus, index);
        }
    }

    #[test]
    fn a_matched_pair_differs_only_in_the_probed_locus() {
        let base = matched_base();
        for locus in 0..DevelopmentalGenome::LOCUS_COUNT {
            let low = base.with_locus(locus, LOW);
            let high = base.with_locus(locus, HIGH);
            let differing = low
                .loci()
                .iter()
                .zip(high.loci())
                .enumerate()
                .filter(|(_, (left, right))| left != right)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(differing, [locus]);
        }
    }

    #[test]
    fn probing_is_deterministic() {
        assert_eq!(pair(0, 3), pair(0, 3));
        assert_eq!(pair(12, 3), pair(12, 3));
    }

    #[test]
    fn scripted_stream_elicits_the_reproductive_signal() {
        let genome = DevelopmentalGenome::uniform(MIDPOINT, PROBE_MUTATION_PPM);
        let actions = actions(3, &genome);
        let positive = actions
            .iter()
            .filter(|action| action.signals[0] > 0)
            .count();
        eprintln!("signal0 > 0 on {positive} of {} decisions", actions.len());
        eprintln!(
            "first signals {:?}",
            actions
                .iter()
                .take(12)
                .map(|action| action.signals)
                .collect::<Vec<_>>()
        );
        assert!(positive * 2 > actions.len());
    }

    #[test]
    fn a_mutation_effect_is_reported_for_every_locus_and_both_directions() {
        let effects = mutation_effects(&[3]);
        assert_eq!(effects.len(), DevelopmentalGenome::LOCUS_COUNT * 2);
        assert!(effects.iter().any(|effect| effect.parameter_change != 0));
    }
}
