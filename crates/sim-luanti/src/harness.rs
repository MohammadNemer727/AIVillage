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
    environment::{BodyProprioception, PrimitiveAction},
    runtime::{AFFORDANCE_ACTIONS, AgentRuntime, ExperimentMode, HAND_CONTACT_FEATURE},
};

use crate::benchmark::{config, observation, random_action};

/// Fixed seeds for every protocol.
pub const SEEDS: [u64; 8] = [7, 19, 43, 91, 131, 173, 211, 251];
/// Experiment slots: the learner's twelve primitives plus "other" for anything else.
pub const ACTION_SLOTS: usize = AFFORDANCE_ACTIONS + 1;
/// Slot of the generic hand pulse.
pub const HAND_PULSE: usize = 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Controller {
    PredictiveBaseline,
    Random,
    AffordanceDiscovery,
}

pub const CONTROLLERS: [Controller; 3] = [
    Controller::PredictiveBaseline,
    Controller::Random,
    Controller::AffordanceDiscovery,
];

/// How the coupled object responds during a phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coupling {
    /// The object stays at its resting contact whatever the founder does.
    Rest,
    /// The target experiment produces the coupling's caused contact on the next observation.
    Contingent,
    /// Contact replays a partner run's recorded contact at the same decision.
    Yoked,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Phase {
    pub name: &'static str,
    pub decisions: u64,
    pub coupling: Coupling,
}

/// The coupled object: which experiment moves it, and how it feels.
#[derive(Clone, Copy, Debug)]
pub struct Object {
    /// Experiment slot the object responds to.
    pub target: usize,
    /// Contact while the target has no consequence.
    pub resting: u16,
    /// Contact after the target experiment while contingent, keyed by seed and decision.
    pub caused: fn(u64, u64) -> u16,
}

/// Every decision of one run.
#[derive(Clone, Debug)]
pub struct Trace {
    pub seed: u64,
    /// Experiment slot chosen at each decision.
    pub slots: Vec<usize>,
    /// Contact delivered after each decision.
    pub contacts: Vec<u16>,
    /// The learner's expected contact after the target experiment, read before each outcome
    /// arrived, or `None` while untried or for controllers without a learner.
    pub expected: Vec<Option<u16>>,
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

/// Runs one seed through every phase. Yoked phases replay `partner`'s contacts.
#[must_use]
pub fn run(
    controller: Controller,
    phases: &[Phase],
    object: Object,
    seed: u64,
    partner: Option<&Trace>,
) -> Trace {
    let mode = if controller == Controller::AffordanceDiscovery {
        ExperimentMode::AffordanceDiscovery
    } else {
        ExperimentMode::PredictiveBaseline
    };
    let mut runtime = AgentRuntime::new_with_mode(seed, 1, config(), mode);
    let mut body = BodyProprioception {
        eye_pitch: 128,
        grounded: 255,
        hand_contact: object.resting,
        ..BodyProprioception::default()
    };
    let mut acceleration = 0;
    let total: u64 = phases.iter().map(|phase| phase.decisions).sum();
    let mut trace = Trace {
        seed,
        slots: Vec::with_capacity(total as usize),
        contacts: Vec::with_capacity(total as usize),
        expected: Vec::with_capacity(total as usize),
    };
    let ranges = phase_ranges(phases);
    for tick in 0..total {
        let index = tick as usize;
        let phase = ranges
            .iter()
            .position(|range| range.contains(&index))
            .expect("tick within protocol");
        let mut sensed = observation(tick, body, body.hand_contact);
        sensed.senses.acceleration = acceleration;
        let transition = runtime.step(sensed).expect("fixture sequence");
        let action = match controller {
            Controller::PredictiveBaseline | Controller::AffordanceDiscovery => transition.action,
            Controller::Random => random_action(seed, tick),
        };
        let chosen = slot(&action);
        let expected = (controller == Controller::AffordanceDiscovery)
            .then(|| runtime.state().affordance.expected_after(object.target))
            .flatten()
            .map(|outcome| outcome[HAND_CONTACT_FEATURE].clamp(0, 255) as u16);
        let contact = match phases[phase].coupling {
            Coupling::Contingent if chosen == object.target => (object.caused)(seed, tick),
            Coupling::Rest | Coupling::Contingent => object.resting,
            Coupling::Yoked => partner.expect("yoked phase needs a partner").contacts[index],
        };
        trace.slots.push(chosen);
        trace.contacts.push(contact);
        trace.expected.push(expected);
        body.hand_contact = contact;
        body.eye_pitch =
            (i32::from(body.eye_pitch) + i32::from(action.look_pitch) / 32).clamp(0, 255) as u16;
        body.grounded = if action.jump > 0 { 0 } else { 255 };
        body.vertical_motion = if action.jump > 0 { 255 } else { 0 };
        acceleration = (action.thrust / 2).clamp(-255, 255);
    }
    trace
}

/// Runs every seed, then yokes each seed to the next seed's run of `source` (as infants are
/// yoked to a partner: replaying a deterministic run's own contacts would reproduce that run).
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
    /// Decisions whose delivered contact differed from rest.
    pub consequences: u32,
    /// Mean distance of the learner's expected target contact from rest, over decisions where
    /// it held an expectation. Zero for controllers without a learner.
    pub anticipation: u32,
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
                    summary.consequences += u32::from(trace.contacts[index] != object.resting);
                    if let Some(expected) = trace.expected[index] {
                        anticipation += u64::from(expected.abs_diff(object.resting));
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
        resting: 0,
        caused: |_, _| 255,
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
        for (index, (slot, contact)) in trace.slots.iter().zip(&trace.contacts).enumerate() {
            let caused = index >= 16 && *slot == HAND_PULSE;
            assert_eq!(*contact, if caused { 255 } else { 0 }, "decision {index}");
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
            assert_eq!(trace.contacts[16..], partner.contacts[16..]);
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
