//! Reusable deterministic harness for infant-style developmental protocols.
//!
//! A protocol is a sequence of phases on one synthetic body. In each phase a coupled object is
//! either at rest, contingent on one target experiment, or yoked to another run's recorded
//! consequences. The harness records every decision, so protocols can report what the infant
//! literature asks for rather than one pooled rate: per-run learning curves, the full mix of
//! experiments in each phase (strategies), and what the learner expected before each outcome
//! arrived (anticipation).
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_panics_doc,
    reason = "compact deterministic experiment harness"
)]

use std::ops::Range;

use serde::Serialize;
use sim_core::{
    RETINAL_RAY_COUNT, RetinalSample,
    environment::{BodyProprioception, PrimitiveAction},
    runtime::{
        AFFORDANCE_ACTIONS, AffordanceConfig, AgentRuntime, ContextMode, ExperimentMode,
        HAND_CONTACT_FEATURE, OutcomeModel, ScoringRule, VISUAL_CHANGE_FEATURE,
    },
};

use sim_kernels::derive;

use crate::benchmark::{config, observation, random_action};

/// Fixed seeds for every protocol.
pub const SEEDS: [u64; 8] = [7, 19, 43, 91, 131, 173, 211, 251];
/// Experiment slots: the learner's twelve primitives plus "other" for anything else.
pub const ACTION_SLOTS: usize = AFFORDANCE_ACTIONS + 1;
/// Slot of the generic hand pulse.
pub const HAND_PULSE: usize = 9;
/// Retinal rays the object covers when it is seen.
const SEEN_RAYS: usize = 3;
/// Broad-band colour of a seen object, as the Luanti test cube's average texture colour.
const SEEN_COLOUR: u16 = 176;
/// Directions a body that turns can face; the object of a facing protocol stands in direction 0.
pub const HEADINGS: usize = 8;
/// Decisions per facing trial; each trial starts facing a seed-chosen direction.
pub const FACING_TRIAL: u64 = 40;
/// Occlusion of an object standing within reach at eye height.
const REACHABLE_OCCLUSION: u16 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Controller {
    PredictiveBaseline,
    Random,
    /// The affordance learner with its default contingency rule.
    AffordanceDiscovery,
    /// The affordance learner scoring experiments by learning progress instead.
    LearningProgress,
    /// The affordance learner judging contingency within the situation its senses report.
    ContextDiscovery,
    /// The affordance learner judging contingency by full outcome distributions (expectiles).
    ExpectileDiscovery,
    /// The affordance learner judging contingency within situations it discovers itself.
    LearnedSituations,
}

impl Controller {
    /// Settings of the affordance learner this controller runs, if it runs one.
    #[must_use]
    pub fn learner(self) -> Option<AffordanceConfig> {
        let (rule, context, outcome_model) = match self {
            Self::PredictiveBaseline | Self::Random => return None,
            Self::AffordanceDiscovery => (
                ScoringRule::Contingency,
                ContextMode::None,
                OutcomeModel::Moments,
            ),
            Self::LearningProgress => (
                ScoringRule::LearningProgress,
                ContextMode::None,
                OutcomeModel::Moments,
            ),
            Self::ContextDiscovery => (
                ScoringRule::Contingency,
                ContextMode::SenseBits,
                OutcomeModel::Moments,
            ),
            Self::ExpectileDiscovery => (
                ScoringRule::Contingency,
                ContextMode::None,
                OutcomeModel::Expectiles,
            ),
            Self::LearnedSituations => (
                ScoringRule::Contingency,
                ContextMode::Learned,
                OutcomeModel::Moments,
            ),
        };
        Some(AffordanceConfig {
            rule,
            context,
            outcome_model,
            ..AffordanceConfig::default()
        })
    }
}

pub const CONTROLLERS: [Controller; 7] = [
    Controller::PredictiveBaseline,
    Controller::Random,
    Controller::AffordanceDiscovery,
    Controller::LearningProgress,
    Controller::ContextDiscovery,
    Controller::ExpectileDiscovery,
    Controller::LearnedSituations,
];

/// How the founder senses the coupled object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sense {
    /// The object's level is the hand-contact amplitude.
    Contact,
    /// The object is seen but never touched: its level is the occlusion (nearness) of the rays
    /// it covers, and a change of level is seen as apparent motion along those rays.
    Sight,
}

/// How the coupled object responds during a phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coupling {
    /// The object stays at its resting level whatever the founder does.
    Rest,
    /// The target experiment produces the object's caused level on the next observation.
    Contingent,
    /// The level replays a partner run's recorded level at the same decision.
    Yoked,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Phase {
    pub name: &'static str,
    pub decisions: u64,
    pub coupling: Coupling,
}

/// The coupled object: which experiment moves it, and how it is sensed.
#[derive(Clone, Copy, Debug)]
pub struct Object {
    /// Experiment slot the object responds to.
    pub target: usize,
    pub sense: Sense,
    /// Level while the target has no consequence.
    pub resting: u16,
    /// Level after the target experiment while contingent, keyed by seed and decision.
    pub caused: fn(u64, u64) -> u16,
    /// Extra decisions before a caused level arrives; zero delivers it with the next
    /// observation.
    pub delay: u64,
    /// The object stands within reach at eye height in one direction, and the body can turn. It
    /// is seen on the eye rays toward it and responds only while the founder faces it. Without
    /// this the object is always in front and the body does not turn.
    pub facing: bool,
}

impl Object {
    /// How strongly the learner expects the target experiment to change what it senses: its
    /// expected contact's distance from rest, or its expected view change.
    fn anticipation(self, expected: [i32; sim_core::runtime::OUTCOME_FEATURES]) -> u16 {
        let value = match self.sense {
            Sense::Contact => (expected[HAND_CONTACT_FEATURE] - i32::from(self.resting)).abs(),
            Sense::Sight => expected[VISUAL_CHANGE_FEATURE],
        };
        value.clamp(0, 255) as u16
    }
}

/// Every decision of one run.
#[derive(Clone, Debug)]
pub struct Trace {
    pub seed: u64,
    /// Experiment slot chosen at each decision.
    pub slots: Vec<usize>,
    /// Object level delivered after each decision.
    pub levels: Vec<u16>,
    /// The learner's anticipated change after the target experiment, read before each outcome
    /// arrived, or `None` while untried or for controllers without a learner.
    pub expected: Vec<Option<u16>>,
    /// Whether the founder faced the object at each decision; always true without facing.
    pub facing: Vec<bool>,
    /// Situations the learner split on by the end of the run, as (sense fact, outcomes seen).
    pub splits: Vec<(usize, u64)>,
}

/// Maps an action to its experiment slot, in the learner's primitive order.
#[must_use]
pub fn slot(action: &PrimitiveAction) -> usize {
    let pulses = [
        action.thrust > 0,
        action.thrust < 0,
        action.turn > 0,
        action.turn < 0,
        action.strafe > 0,
        action.strafe < 0,
        action.look_pitch > 0,
        action.look_pitch < 0,
        action.jump > 0,
        action.hand_pulse > 0,
        action.jaw > 0,
        action.acoustic > 0,
    ];
    pulses
        .iter()
        .position(|pulse| *pulse)
        .unwrap_or(AFFORDANCE_ACTIONS)
}

/// Decision ranges of each phase.
#[must_use]
pub fn phase_ranges(phases: &[Phase]) -> Vec<Range<usize>> {
    let mut start = 0;
    phases
        .iter()
        .map(|phase| {
            let end = start + phase.decisions as usize;
            let range = start..end;
            start = end;
            range
        })
        .collect()
}

/// Draws the object on the eye rays: toward it at eye height for a facing object, or straight
/// ahead for an object that is only seen.
fn show_object(
    retinal: &mut [RetinalSample; RETINAL_RAY_COUNT],
    object: Object,
    heading: usize,
    level: u16,
    previous_level: u16,
) {
    if object.facing {
        // Increasing ray index turns left, and each heading step turned left puts the
        // object one step further to the right.
        let rays = RETINAL_RAY_COUNT;
        let centre = (rays - heading * (rays / HEADINGS)) % rays;
        for offset in [rays - 1, 0, 1] {
            let ray = &mut retinal[(centre + offset) % rays];
            ray.red = SEEN_COLOUR;
            ray.green = SEEN_COLOUR;
            ray.blue = SEEN_COLOUR;
            ray.occlusion = match object.sense {
                Sense::Contact => REACHABLE_OCCLUSION,
                Sense::Sight => level,
            };
        }
    } else if object.sense == Sense::Sight {
        for ray in &mut retinal[..SEEN_RAYS] {
            ray.red = SEEN_COLOUR;
            ray.green = SEEN_COLOUR;
            ray.blue = SEEN_COLOUR;
            ray.occlusion = level;
            ray.motion = (i32::from(level) - i32::from(previous_level)) as i16;
        }
    }
}

/// Runs one seed through every phase. Yoked phases replay `partner`'s levels.
#[must_use]
pub fn run(
    controller: Controller,
    phases: &[Phase],
    object: Object,
    seed: u64,
    partner: Option<&Trace>,
) -> Trace {
    let mut runtime = match controller.learner() {
        Some(learner) => AgentRuntime::new_with_affordance(seed, 1, config(), learner)
            .expect("default learner settings are valid"),
        None => AgentRuntime::new_with_mode(seed, 1, config(), ExperimentMode::PredictiveBaseline),
    };
    let mut body = BodyProprioception {
        eye_pitch: 128,
        grounded: 255,
        ..BodyProprioception::default()
    };
    let mut level = object.resting;
    let mut previous_level = level;
    let mut acceleration = 0;
    let total: u64 = phases.iter().map(|phase| phase.decisions).sum();
    let mut scheduled = vec![None; (total + object.delay) as usize];
    let mut trace = Trace {
        seed,
        slots: Vec::with_capacity(total as usize),
        levels: Vec::with_capacity(total as usize),
        expected: Vec::with_capacity(total as usize),
        facing: Vec::with_capacity(total as usize),
        splits: Vec::new(),
    };
    let ranges = phase_ranges(phases);
    let mut heading = 0;
    for tick in 0..total {
        let index = tick as usize;
        let phase = ranges
            .iter()
            .position(|range| range.contains(&index))
            .expect("tick within protocol");
        if object.facing && tick % FACING_TRIAL == 0 {
            heading =
                (derive(seed, tick / FACING_TRIAL, 0, 0x4845_4144) % HEADINGS as u64) as usize;
        }
        let facing = !object.facing || heading == 0;
        match object.sense {
            Sense::Contact => body.hand_contact = level,
            Sense::Sight => body.hand_contact = 0,
        }
        let mut sensed = observation(tick, body, body.hand_contact);
        sensed.senses.acceleration = acceleration;
        show_object(
            &mut sensed.senses.retinal,
            object,
            heading,
            level,
            previous_level,
        );
        let transition = runtime.step(sensed).expect("fixture sequence");
        let action = match controller {
            Controller::Random => random_action(seed, tick),
            _ => transition.action,
        };
        let chosen = slot(&action);
        let expected = controller
            .learner()
            .is_some()
            .then(|| runtime.state().affordance.expected_after(object.target))
            .flatten()
            .map(|outcome| object.anticipation(outcome));
        if phases[phase].coupling == Coupling::Contingent && chosen == object.target && facing {
            scheduled[index + object.delay as usize] = Some((object.caused)(seed, tick));
        }
        previous_level = level;
        level = match phases[phase].coupling {
            Coupling::Yoked => partner.expect("yoked phase needs a partner").levels[index],
            Coupling::Rest | Coupling::Contingent => scheduled[index].unwrap_or(object.resting),
        };
        trace.slots.push(chosen);
        trace.levels.push(level);
        trace.expected.push(expected);
        trace.facing.push(facing);
        if object.facing && action.turn != 0 {
            heading = if action.turn > 0 {
                (heading + 1) % HEADINGS
            } else {
                (heading + HEADINGS - 1) % HEADINGS
            };
        }
        body.eye_pitch =
            (i32::from(body.eye_pitch) + i32::from(action.look_pitch) / 32).clamp(0, 255) as u16;
        body.grounded = if action.jump > 0 { 0 } else { 255 };
        body.vertical_motion = if action.jump > 0 { 255 } else { 0 };
        acceleration = (action.thrust / 2).clamp(-255, 255);
    }
    trace.splits = runtime
        .state()
        .affordance
        .splits
        .iter()
        .map(|split| (usize::from(split.fact), split.at))
        .collect();
    trace
}

/// Runs every seed, then yokes each seed to the next seed's run of `source` (as infants are
/// yoked to a partner: replaying a deterministic run's own levels would reproduce that run).
#[must_use]
pub fn run_yoked(
    controller: Controller,
    phases: &[Phase],
    object: Object,
    source: &[Trace],
) -> Vec<Trace> {
    SEEDS
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            let partner = &source[(index + 1) % source.len()];
            run(controller, phases, object, *seed, Some(partner))
        })
        .collect()
}

/// What one phase of one or more runs looked like.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct PhaseSummary {
    pub decisions: u32,
    /// Decisions per experiment slot: the strategy mix.
    pub slot_counts: [u32; ACTION_SLOTS],
    /// Decisions whose delivered level differed from rest.
    pub consequences: u32,
    /// Mean anticipated change after the target experiment, over decisions where the learner
    /// held an expectation. Zero for controllers without a learner.
    pub anticipation: u32,
    /// Decisions made while facing the object.
    pub facing_decisions: u32,
    /// Target experiments made while facing the object.
    pub target_while_facing: u32,
}

impl PhaseSummary {
    #[must_use]
    pub fn target(&self, object: Object) -> u32 {
        self.slot_counts[object.target]
    }
}

/// Summarises each phase over a set of runs.
#[must_use]
pub fn summarize(phases: &[Phase], object: Object, traces: &[Trace]) -> Vec<PhaseSummary> {
    phase_ranges(phases)
        .into_iter()
        .map(|range| {
            let mut summary = PhaseSummary::default();
            let mut anticipation = 0_u64;
            let mut expectations = 0_u64;
            for trace in traces {
                for index in range.clone() {
                    summary.decisions += 1;
                    summary.slot_counts[trace.slots[index]] += 1;
                    summary.consequences += u32::from(trace.levels[index] != object.resting);
                    if trace.facing[index] {
                        summary.facing_decisions += 1;
                        summary.target_while_facing +=
                            u32::from(trace.slots[index] == object.target);
                    }
                    if let Some(expected) = trace.expected[index] {
                        anticipation += u64::from(expected);
                        expectations += 1;
                    }
                }
            }
            summary.anticipation = (anticipation / expectations.max(1)) as u32;
            summary
        })
        .collect()
}

/// Target experiments per block of `block` decisions: one run's learning curve.
#[must_use]
pub fn curve(trace: &Trace, object: Object, block: usize) -> Vec<u32> {
    trace
        .slots
        .chunks(block)
        .map(|chunk| chunk.iter().filter(|slot| **slot == object.target).count() as u32)
        .collect()
}

/// Rate per 100 decisions, to one decimal place.
#[must_use]
pub fn rate(count: u32, decisions: u32) -> String {
    let tenths = u64::from(count) * 1_000 / u64::from(decisions.max(1));
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHASES: [Phase; 2] = [
        Phase {
            name: "rest",
            decisions: 16,
            coupling: Coupling::Rest,
        },
        Phase {
            name: "contingent",
            decisions: 32,
            coupling: Coupling::Contingent,
        },
    ];
    const OBJECT: Object = Object {
        target: HAND_PULSE,
        sense: Sense::Contact,
        resting: 0,
        caused: |_, _| 255,
        delay: 0,
        facing: false,
    };

    #[test]
    fn slots_follow_the_learner_primitive_order() {
        let mut action = PrimitiveAction::default();
        assert_eq!(slot(&action), AFFORDANCE_ACTIONS);
        action.hand_pulse = 256;
        assert_eq!(slot(&action), HAND_PULSE);
        action.thrust = -1;
        assert_eq!(slot(&action), 1);
    }

    #[test]
    fn contact_follows_only_the_target_while_contingent() {
        let trace = run(Controller::Random, &PHASES, OBJECT, 7, None);
        for (index, (slot, level)) in trace.slots.iter().zip(&trace.levels).enumerate() {
            let caused = index >= 16 && *slot == HAND_PULSE;
            assert_eq!(*level, if caused { 255 } else { 0 }, "decision {index}");
        }
    }

    #[test]
    fn delayed_consequences_arrive_after_the_delay() {
        let object = Object { delay: 2, ..OBJECT };
        let trace = run(Controller::Random, &PHASES, object, 7, None);
        for index in 0..trace.levels.len() {
            let caused = index >= 18 && trace.slots[index - 2] == HAND_PULSE;
            assert_eq!(
                trace.levels[index],
                if caused { 255 } else { 0 },
                "decision {index}"
            );
        }
    }

    #[test]
    fn runs_are_deterministic_and_yoking_replays_the_partner() {
        let phases = [
            PHASES[0],
            Phase {
                coupling: Coupling::Yoked,
                ..PHASES[1]
            },
        ];
        let source: Vec<_> = SEEDS
            .iter()
            .map(|seed| run(Controller::Random, &PHASES, OBJECT, *seed, None))
            .collect();
        let yoked = run_yoked(Controller::Random, &phases, OBJECT, &source);
        for (index, trace) in yoked.iter().enumerate() {
            let partner = &source[(index + 1) % source.len()];
            assert_eq!(trace.levels[16..], partner.levels[16..]);
        }
        let again = run(Controller::AffordanceDiscovery, &PHASES, OBJECT, 7, None);
        let first = run(Controller::AffordanceDiscovery, &PHASES, OBJECT, 7, None);
        assert_eq!(again.slots, first.slots);
        assert_eq!(again.expected, first.expected);
    }

    #[test]
    fn summaries_partition_every_decision() {
        let trace = run(Controller::Random, &PHASES, OBJECT, 19, None);
        let summaries = summarize(&PHASES, OBJECT, std::slice::from_ref(&trace));
        assert_eq!(summaries[0].decisions, 16);
        assert_eq!(summaries[1].decisions, 32);
        assert_eq!(summaries[0].consequences, 0);
        assert_eq!(summaries[1].consequences, summaries[1].target(OBJECT));
        let curve = curve(&trace, OBJECT, 8);
        assert_eq!(
            curve.iter().sum::<u32>(),
            summaries.iter().map(|s| s.target(OBJECT)).sum::<u32>()
        );
    }
}
