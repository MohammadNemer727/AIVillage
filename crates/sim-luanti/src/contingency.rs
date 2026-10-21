//! Deterministic infant-style contingency test (after Rovee-Collier's mobile paradigm).
//!
//! A baby whose kicks shake a mobile kicks more; when the ribbon is removed the kicking falls
//! back; a baby shown a mobile that moves on its own does not kick more. Here the "kick" is the
//! generic hand pulse and the "mobile" is a nearby test object whose only consequence is bounded
//! contact feedback. Nothing names the object, and no controller receives a reward.
//!
//! Each run has three equal phases:
//! - baseline: the object rests at its paradigm's resting contact, whatever the founder does;
//! - middle: contingent (a hand pulse changes the next contact) or yoked (contact replays another
//!   seed's contingent timeline for the same controller, regardless of what the founder does);
//! - extinction: the object rests again.
//!
//! The paradigms follow Xu & Triesch (Developmental Science, 2026), which shows that simpler
//! contingency detectors fail when an action removes an effect or only changes its variability:
//! - standard: no contact at rest; a pulse produces contact;
//! - inverted: constant contact at rest; a pulse removes it;
//! - bimodal: half contact at rest; a pulse produces full or no contact by a fair coin, so the
//!   average outcome is unchanged and only its spread depends on the action.
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::missing_panics_doc,
    reason = "compact deterministic experiment harness"
)]

use serde::Serialize;
use sim_core::{
    environment::{BodyProprioception, PrimitiveAction},
    runtime::{AgentRuntime, ExperimentMode},
};
use sim_kernels::derive;

use crate::benchmark::{config, observation, random_action};

const SEEDS: [u64; 4] = [7, 19, 43, 91];
const PHASE: u64 = 96;
const CONTACT: u16 = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Controller {
    PredictiveBaseline,
    Random,
    AffordanceDiscovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Paradigm {
    Standard,
    Inverted,
    Bimodal,
}

impl Paradigm {
    /// Contact while the hand pulse has no consequence.
    const fn resting(self) -> u16 {
        match self {
            Self::Standard => 0,
            Self::Inverted => CONTACT,
            Self::Bimodal => CONTACT / 2,
        }
    }

    /// Contact following a hand pulse while the object is connected.
    fn pulsed(self, seed: u64, tick: u64) -> u16 {
        match self {
            Self::Standard => CONTACT,
            Self::Inverted => 0,
            Self::Bimodal => {
                if derive(seed, tick, 0, 0x4249_4D4F) & 1 == 0 {
                    CONTACT
                } else {
                    0
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Contingent,
    Yoked,
}

/// Hand pulses summed over all seeds in each phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Row {
    pub controller: Controller,
    pub paradigm: Paradigm,
    pub condition: Condition,
    pub baseline_pulses: u32,
    pub middle_pulses: u32,
    pub extinction_pulses: u32,
    /// Middle-phase decisions whose contact differed from rest.
    pub middle_consequences: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Verdict {
    pub controller: Controller,
    pub paradigm: Paradigm,
    /// Contingent pulsing is at least 1.5x both its own baseline and the yoked middle phase,
    /// and falls again in extinction.
    pub shows_contingency: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContingencyReport {
    pub schema_version: u16,
    pub seeds: [u64; 4],
    pub phase_decisions: u64,
    pub rows: Vec<Row>,
    pub verdicts: Vec<Verdict>,
}

struct RunResult {
    pulses: [u32; 3],
    contacts: Vec<u16>,
}

/// Runs one seed. `yoked` replays a recorded middle-phase contact timeline; `None` makes contact
/// follow the founder's own hand pulses.
fn run_seed(
    controller: Controller,
    paradigm: Paradigm,
    seed: u64,
    yoked: Option<&[u16]>,
) -> RunResult {
    let mode = if controller == Controller::AffordanceDiscovery {
        ExperimentMode::AffordanceDiscovery
    } else {
        ExperimentMode::PredictiveBaseline
    };
    let mut runtime = AgentRuntime::new_with_mode(seed, 1, config(), mode);
    let mut body = BodyProprioception {
        eye_pitch: 128,
        grounded: 255,
        hand_contact: paradigm.resting(),
        ..BodyProprioception::default()
    };
    let mut acceleration = 0;
    let mut pulses = [0; 3];
    let mut contacts = Vec::new();
    for tick in 0..3 * PHASE {
        let phase = (tick / PHASE) as usize;
        let mut sensed = observation(tick, body, body.hand_contact);
        sensed.senses.acceleration = acceleration;
        let transition = runtime.step(sensed).expect("fixture sequence");
        let action: PrimitiveAction = match controller {
            Controller::PredictiveBaseline | Controller::AffordanceDiscovery => transition.action,
            Controller::Random => random_action(seed, tick),
        };
        let pulse = action.hand_pulse > 0;
        pulses[phase] += u32::from(pulse);
        let contact = match (phase, yoked) {
            (1, Some(timeline)) => timeline[(tick - PHASE) as usize],
            (1, None) if pulse => paradigm.pulsed(seed, tick),
            _ => paradigm.resting(),
        };
        if phase == 1 {
            contacts.push(contact);
        }
        body.hand_contact = contact;
        body.eye_pitch =
            (i32::from(body.eye_pitch) + i32::from(action.look_pitch) / 32).clamp(0, 255) as u16;
        body.grounded = if action.jump > 0 { 0 } else { 255 };
        body.vertical_motion = if action.jump > 0 { 255 } else { 0 };
        acceleration = (action.thrust / 2).clamp(-255, 255);
    }
    RunResult { pulses, contacts }
}

fn row(
    controller: Controller,
    paradigm: Paradigm,
    condition: Condition,
    runs: &[RunResult],
) -> Row {
    let sum = |phase: usize| runs.iter().map(|run| run.pulses[phase]).sum();
    Row {
        controller,
        paradigm,
        condition,
        baseline_pulses: sum(0),
        middle_pulses: sum(1),
        extinction_pulses: sum(2),
        middle_consequences: runs
            .iter()
            .flat_map(|run| &run.contacts)
            .filter(|contact| **contact != paradigm.resting())
            .count() as u32,
    }
}

/// Runs every paradigm in both conditions for every controller over all seeds.
#[must_use]
pub fn run() -> ContingencyReport {
    let mut rows = Vec::new();
    let mut verdicts = Vec::new();
    for paradigm in [Paradigm::Standard, Paradigm::Inverted, Paradigm::Bimodal] {
        for controller in [
            Controller::PredictiveBaseline,
            Controller::Random,
            Controller::AffordanceDiscovery,
        ] {
            let contingent: Vec<_> = SEEDS
                .iter()
                .map(|seed| run_seed(controller, paradigm, *seed, None))
                .collect();
            // As with infants, each run is yoked to a different partner's timeline: replaying a
            // deterministic run's own timeline would reproduce that run exactly.
            let yoked: Vec<_> = SEEDS
                .iter()
                .enumerate()
                .map(|(index, seed)| {
                    let partner = &contingent[(index + 1) % contingent.len()];
                    run_seed(controller, paradigm, *seed, Some(&partner.contacts))
                })
                .collect();
            let contingent = row(controller, paradigm, Condition::Contingent, &contingent);
            let yoked = row(controller, paradigm, Condition::Yoked, &yoked);
            let middle = contingent.middle_pulses;
            verdicts.push(Verdict {
                controller,
                paradigm,
                shows_contingency: middle > 0
                    && 2 * middle >= 3 * contingent.baseline_pulses
                    && 2 * middle >= 3 * yoked.middle_pulses
                    && contingent.extinction_pulses < middle,
            });
            rows.push(contingent);
            rows.push(yoked);
        }
    }
    ContingencyReport {
        schema_version: 2,
        seeds: SEEDS,
        phase_decisions: PHASE,
        rows,
        verdicts,
    }
}

/// Hand pulses per 100 decisions, to one decimal place.
fn rate(pulses: u32, decisions: u64) -> String {
    let tenths = u64::from(pulses) * 1_000 / decisions;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Compact truthful Markdown rendering for checked-in experiment evidence.
#[must_use]
pub fn markdown(report: &ContingencyReport) -> String {
    let decisions = report.phase_decisions * report.seeds.len() as u64;
    let mut text = format!(
        "# Contingency test\n\nInfant mobile paradigm on deterministic synthetic fixtures: hand \
         pulses per 100 decisions over {} seeds, {} decisions per phase. In the contingent \
         condition a hand pulse changes the next contact; in the yoked condition contact replays \
         the contingent timeline of another seed regardless of action. Standard: a pulse produces \
         contact. Inverted: a pulse removes constant contact. Bimodal: a pulse turns half contact \
         into full or none by a fair coin, leaving the average unchanged. No controller receives \
         a reward.\n\n\
         | paradigm | controller | condition | baseline | middle | extinction | consequences |\n\
         |---|---|---|---:|---:|---:|---:|\n",
        report.seeds.len(),
        report.phase_decisions
    );
    for row in &report.rows {
        text.push_str(&format!(
            "| {:?} | {:?} | {:?} | {} | {} | {} | {} |\n",
            row.paradigm,
            row.controller,
            row.condition,
            rate(row.baseline_pulses, decisions),
            rate(row.middle_pulses, decisions),
            rate(row.extinction_pulses, decisions),
            row.middle_consequences
        ));
    }
    text.push_str("\n| paradigm | controller | shows contingency |\n|---|---|---|\n");
    for verdict in &report.verdicts {
        text.push_str(&format!(
            "| {:?} | {:?} | {} |\n",
            verdict.paradigm, verdict.controller, verdict.shows_contingency
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(report: &ContingencyReport, paradigm: Paradigm) -> &Verdict {
        report
            .verdicts
            .iter()
            .find(|verdict| {
                verdict.controller == Controller::AffordanceDiscovery
                    && verdict.paradigm == paradigm
            })
            .expect("affordance controller")
    }

    #[test]
    fn report_is_deterministic() {
        let a = run();
        let b = run();
        assert_eq!(a.rows, b.rows);
        assert_eq!(a.verdicts, b.verdicts);
        assert_eq!(a.rows.len(), 18);
    }

    #[test]
    fn yoked_condition_replays_the_contingent_consequences() {
        for pair in run().rows.chunks(2) {
            assert_eq!(pair[0].middle_consequences, pair[1].middle_consequences);
        }
    }

    #[test]
    fn controllers_blind_to_consequences_show_no_contingency() {
        let report = run();
        for verdict in &report.verdicts {
            if verdict.controller != Controller::AffordanceDiscovery {
                assert!(!verdict.shows_contingency, "{verdict:?}");
            }
        }
    }

    #[test]
    fn affordance_discovery_shows_contingency() {
        let report = run();
        for paradigm in [Paradigm::Standard, Paradigm::Inverted] {
            assert!(
                verdict(&report, paradigm).shows_contingency,
                "{paradigm:?}: {:?}",
                report.rows
            );
        }
    }
}
