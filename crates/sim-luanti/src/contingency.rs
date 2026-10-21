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
//!   seed's contingent run for the same controller, regardless of what the founder does);
//! - extinction: the object rests again. Its first half is reported separately because infants
//!   often pulse harder right after disconnection (the extinction burst); the verdict asks only
//!   that the second half falls below the connected phase.
//!
//! The paradigms follow Xu & Triesch (Developmental Science, 2026), which shows that simpler
//! contingency detectors fail when an action removes an effect or only changes its variability:
//! - standard: no contact at rest; a pulse produces contact;
//! - inverted: constant contact at rest; a pulse removes it;
//! - bimodal: half contact at rest; a pulse produces full or no contact by a fair coin, so the
//!   average outcome is unchanged and only its spread depends on the action.
//!
//! A fourth paradigm has no touch at all, like an infant watching a mobile it cannot reach:
//! - seen: the object is visible nearby at rest; a pulse pushes it away, so the rays it covers
//!   see it recede and its nearness drop.
//!
//! Beyond pooled rates, the report follows the infant literature: per-seed learning curves
//! (Fujihira et al., 2025), the strategy mix in each phase (Spisak et al., 2026), and the
//! learner's anticipated contact before each outcome (Rosales et al., 2025).
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    clippy::missing_panics_doc,
    reason = "compact deterministic experiment harness"
)]

use serde::Serialize;
use sim_kernels::derive;

use crate::harness::{
    self, CONTROLLERS, Controller, Coupling, HAND_PULSE, Object, Phase, PhaseSummary, SEEDS, Sense,
    Trace, rate,
};

const PHASE: u64 = 96;
const CURVE_BLOCK: usize = 24;
const CONTACT: u16 = 255;
/// Delays, in decisions, between a standard-paradigm hand pulse and its contact. Infants learn
/// with short delays but not at about three seconds (Millar, 1972).
pub const DELAYS: [u64; 5] = [0, 1, 2, 4, 8];

const CONTINGENT: [Phase; 4] = [
    Phase {
        name: "baseline",
        decisions: PHASE,
        coupling: Coupling::Rest,
    },
    Phase {
        name: "connected",
        decisions: PHASE,
        coupling: Coupling::Contingent,
    },
    Phase {
        name: "early extinction",
        decisions: PHASE / 2,
        coupling: Coupling::Rest,
    },
    Phase {
        name: "late extinction",
        decisions: PHASE / 2,
        coupling: Coupling::Rest,
    },
];
const YOKED: [Phase; 4] = [
    CONTINGENT[0],
    Phase {
        name: "yoked",
        decisions: PHASE,
        coupling: Coupling::Yoked,
    },
    CONTINGENT[2],
    CONTINGENT[3],
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Paradigm {
    Standard,
    Inverted,
    Bimodal,
    Seen,
}

pub const PARADIGMS: [Paradigm; 4] = [
    Paradigm::Standard,
    Paradigm::Inverted,
    Paradigm::Bimodal,
    Paradigm::Seen,
];

impl Paradigm {
    #[must_use]
    pub fn object(self) -> Object {
        match self {
            Self::Standard => Object {
                target: HAND_PULSE,
                sense: Sense::Contact,
                resting: 0,
                caused: |_, _| CONTACT,
                delay: 0,
            },
            Self::Inverted => Object {
                target: HAND_PULSE,
                sense: Sense::Contact,
                resting: CONTACT,
                caused: |_, _| 0,
                delay: 0,
            },
            Self::Bimodal => Object {
                target: HAND_PULSE,
                sense: Sense::Contact,
                resting: CONTACT / 2,
                caused: |seed, tick| {
                    if derive(seed, tick, 0, 0x4249_4D4F) & 1 == 0 {
                        CONTACT
                    } else {
                        0
                    }
                },
                delay: 0,
            },
            Self::Seen => Object {
                target: HAND_PULSE,
                sense: Sense::Sight,
                resting: 160,
                caused: |_, _| 64,
                delay: 0,
            },
        }
    }
}

/// One seed's target experiments per block of decisions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedCurve {
    pub seed: u64,
    pub contingent: Vec<u32>,
    pub yoked: Vec<u32>,
    pub shows_contingency: bool,
}

/// One paradigm for one controller over every seed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Cell {
    pub paradigm: Paradigm,
    pub controller: Controller,
    /// Pooled phase summaries of the contingent runs.
    pub contingent: Vec<PhaseSummary>,
    /// Pooled phase summaries of the yoked runs.
    pub yoked: Vec<PhaseSummary>,
    /// The pooled runs pass the contingency criterion.
    pub shows_contingency: bool,
    /// Seeds whose own contingent and yoked runs pass the criterion.
    pub seeds_passing: u32,
    pub curves: Vec<SeedCurve>,
}

/// The standard paradigm with its contact delayed by `delay` extra decisions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DelayCell {
    pub delay: u64,
    pub cell: Cell,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContingencyReport {
    pub schema_version: u16,
    pub seeds: [u64; 8],
    pub phases: [Phase; 4],
    pub curve_block: usize,
    pub cells: Vec<Cell>,
    pub delays: Vec<DelayCell>,
}

impl ContingencyReport {
    #[must_use]
    pub fn cell(&self, paradigm: Paradigm, controller: Controller) -> &Cell {
        self.cells
            .iter()
            .find(|cell| cell.paradigm == paradigm && cell.controller == controller)
            .expect("every paradigm and controller is run")
    }
}

/// Connected-phase target experiments are at least 1.5x both baseline and the yoked middle
/// phase, and their rate falls again by late extinction. Early extinction is not judged, so an
/// extinction burst does not count against learning.
fn shows_contingency(contingent: &[PhaseSummary], yoked: &[PhaseSummary], object: Object) -> bool {
    let baseline = contingent[0].target(object);
    let middle = contingent[1].target(object);
    let late = contingent[3].target(object);
    middle > 0
        && 2 * middle >= 3 * baseline
        && 2 * middle >= 3 * yoked[1].target(object)
        && late * contingent[1].decisions < middle * contingent[3].decisions
}

fn cell(paradigm: Paradigm, object: Object, controller: Controller) -> Cell {
    let contingent: Vec<Trace> = SEEDS
        .iter()
        .map(|seed| harness::run(controller, &CONTINGENT, object, *seed, None))
        .collect();
    let yoked = harness::run_yoked(controller, &YOKED, object, &contingent);
    let curves: Vec<_> = contingent
        .iter()
        .zip(&yoked)
        .map(|(contingent, yoked)| SeedCurve {
            seed: contingent.seed,
            contingent: harness::curve(contingent, object, CURVE_BLOCK),
            yoked: harness::curve(yoked, object, CURVE_BLOCK),
            shows_contingency: shows_contingency(
                &harness::summarize(&CONTINGENT, object, std::slice::from_ref(contingent)),
                &harness::summarize(&YOKED, object, std::slice::from_ref(yoked)),
                object,
            ),
        })
        .collect();
    let contingent = harness::summarize(&CONTINGENT, object, &contingent);
    let yoked = harness::summarize(&YOKED, object, &yoked);
    Cell {
        paradigm,
        controller,
        shows_contingency: shows_contingency(&contingent, &yoked, object),
        seeds_passing: curves
            .iter()
            .filter(|curve| curve.shows_contingency)
            .count() as u32,
        contingent,
        yoked,
        curves,
    }
}

/// Runs every paradigm in both conditions for every controller over all seeds.
#[must_use]
pub fn run() -> ContingencyReport {
    let cells = PARADIGMS
        .iter()
        .flat_map(|paradigm| {
            CONTROLLERS
                .iter()
                .map(|controller| cell(*paradigm, paradigm.object(), *controller))
        })
        .collect();
    let delays = DELAYS
        .iter()
        .flat_map(|delay| {
            [
                Controller::Random,
                Controller::AffordanceDiscovery,
                Controller::LearningProgress,
            ]
            .map(|controller| DelayCell {
                delay: *delay,
                cell: cell(
                    Paradigm::Standard,
                    Object {
                        delay: *delay,
                        ..Paradigm::Standard.object()
                    },
                    controller,
                ),
            })
        })
        .collect();
    ContingencyReport {
        schema_version: 4,
        seeds: SEEDS,
        phases: CONTINGENT,
        curve_block: CURVE_BLOCK,
        cells,
        delays,
    }
}

/// Experiment groups shown in the strategy table, as (label, slots).
const STRATEGY_GROUPS: [(&str, &[usize]); 8] = [
    ("hand", &[HAND_PULSE]),
    ("jump", &[8]),
    ("thrust", &[0, 1]),
    ("turn", &[2, 3]),
    ("strafe", &[4, 5]),
    ("look", &[6, 7]),
    ("voice", &[10, 11]),
    ("other", &[12]),
];

fn joined(values: impl IntoIterator<Item = String>, separator: &str) -> String {
    values.into_iter().collect::<Vec<_>>().join(separator)
}

/// Compact truthful Markdown rendering for checked-in experiment evidence.
#[must_use]
pub fn markdown(report: &ContingencyReport) -> String {
    let mut text = format!(
        "# Contingency test\n\n\
         Infant mobile paradigm on deterministic synthetic fixtures over {} seeds, {} decisions \
         per phase. In the contingent condition a hand pulse changes the next contact; in the \
         yoked condition contact replays another seed's contingent run regardless of action. \
         Standard: a pulse produces contact. Inverted: a pulse removes constant contact. \
         Bimodal: a pulse turns half contact into full or none by a fair coin, leaving the \
         average unchanged. No controller receives a reward.\n\n\
         ## Hand pulses per 100 decisions\n\n\
         Anticipation is the learner's expected contact after a hand pulse, as distance from \
         rest (0–255), averaged per phase before each outcome arrived. Consequences counts \
         middle-phase decisions whose contact differed from rest.\n\n\
         | paradigm | controller | condition | baseline | middle | early extinction | late extinction | consequences | anticipation |\n\
         |---|---|---|---:|---:|---:|---:|---:|---|\n",
        report.seeds.len(),
        report.phases[0].decisions
    );
    for cell in &report.cells {
        let object = cell.paradigm.object();
        for (condition, phases) in [("Contingent", &cell.contingent), ("Yoked", &cell.yoked)] {
            text.push_str(&format!(
                "| {:?} | {:?} | {condition} | {} | {} | {} | {} | {} | {} |\n",
                cell.paradigm,
                cell.controller,
                rate(phases[0].target(object), phases[0].decisions),
                rate(phases[1].target(object), phases[1].decisions),
                rate(phases[2].target(object), phases[2].decisions),
                rate(phases[3].target(object), phases[3].decisions),
                phases[1].consequences,
                joined(phases.iter().map(|p| p.anticipation.to_string()), " → "),
            ));
        }
    }
    text.push_str(
        "\n## Verdicts\n\nA run shows contingency when connected-phase pulses are at least 1.5× \
         both baseline and the yoked middle phase, and their rate falls below the connected rate by \
         late extinction. Early extinction is not judged, so an extinction burst does not count \
         against learning.\n\n\
         | paradigm | controller | pooled | seeds passing |\n|---|---|---|---:|\n",
    );
    for cell in &report.cells {
        text.push_str(&format!(
            "| {:?} | {:?} | {} | {}/{} |\n",
            cell.paradigm,
            cell.controller,
            cell.shows_contingency,
            cell.seeds_passing,
            cell.curves.len()
        ));
    }
    text.push_str(&delays_markdown(report));
    text.push_str(&curves_markdown(report));
    text.push_str(&strategy_markdown(report));
    text
}

/// Standard-paradigm results with the contact delayed.
fn delays_markdown(report: &ContingencyReport) -> String {
    let mut text = String::from(
        "\n## Delayed consequences\n\nThe standard paradigm with contact arriving `delay` \
         decisions later than the next observation. Hand pulses per 100 decisions; \
         anticipation is the connected-phase mean.\n\n\
         | delay | controller | baseline | connected | early extinction | late extinction | yoked | anticipation | seeds passing |\n\
         |---:|---|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    let object = Paradigm::Standard.object();
    for row in &report.delays {
        let phases = &row.cell.contingent;
        let yoked = &row.cell.yoked[1];
        text.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} | {} | {}/{} |\n",
            row.delay,
            row.cell.controller,
            rate(phases[0].target(object), phases[0].decisions),
            rate(phases[1].target(object), phases[1].decisions),
            rate(phases[2].target(object), phases[2].decisions),
            rate(phases[3].target(object), phases[3].decisions),
            rate(yoked.target(object), yoked.decisions),
            phases[1].anticipation,
            row.cell.seeds_passing,
            row.cell.curves.len()
        ));
    }
    text
}

/// Per-seed learning curves of the affordance learner.
fn curves_markdown(report: &ContingencyReport) -> String {
    let mut text = String::new();
    let blocks_per_phase = report.phases[0].decisions as usize / report.curve_block;
    text.push_str(&format!(
        "\n## AffordanceDiscovery learning curves\n\nHand pulses per {} decisions for each seed, \
         phases separated by `|`.\n\n\
         | paradigm | seed | contingent | yoked | passes |\n|---|---:|---|---|---|\n",
        report.curve_block
    ));
    let curve = |blocks: &[u32]| {
        joined(
            blocks
                .chunks(blocks_per_phase)
                .map(|phase| joined(phase.iter().map(u32::to_string), " ")),
            " | ",
        )
    };
    for paradigm in PARADIGMS {
        for seed in &report
            .cell(paradigm, Controller::AffordanceDiscovery)
            .curves
        {
            text.push_str(&format!(
                "| {paradigm:?} | {} | {} | {} | {} |\n",
                seed.seed,
                curve(&seed.contingent),
                curve(&seed.yoked),
                seed.shows_contingency
            ));
        }
    }
    text
}

/// Strategy mix of the affordance learner in each contingent-condition phase.
fn strategy_markdown(report: &ContingencyReport) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "\n## AffordanceDiscovery strategy mix\n\nShare of decisions per experiment in each \
         contingent-condition phase, in percent.\n\n| paradigm | phase | {} |\n|---|---|{}\n",
        joined(
            STRATEGY_GROUPS.iter().map(|(label, _)| (*label).to_owned()),
            " | "
        ),
        "---:|".repeat(STRATEGY_GROUPS.len())
    ));
    for paradigm in PARADIGMS {
        let cell = report.cell(paradigm, Controller::AffordanceDiscovery);
        for (phase, summary) in report.phases.iter().zip(&cell.contingent) {
            let shares = STRATEGY_GROUPS.iter().map(|(_, slots)| {
                let count: u32 = slots.iter().map(|slot| summary.slot_counts[*slot]).sum();
                (count * 100 / summary.decisions.max(1)).to_string()
            });
            text.push_str(&format!(
                "| {paradigm:?} | {} | {} |\n",
                phase.name,
                joined(shares, " | ")
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_deterministic_and_complete() {
        let a = run();
        let b = run();
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.cells.len(), PARADIGMS.len() * CONTROLLERS.len());
        assert!(a.cells.iter().all(|cell| cell.curves.len() == SEEDS.len()));
    }

    #[test]
    fn yoked_condition_replays_the_contingent_consequences() {
        for cell in run().cells {
            assert_eq!(cell.contingent[1].consequences, cell.yoked[1].consequences);
        }
    }

    #[test]
    fn controllers_blind_to_consequences_show_no_contingency() {
        for cell in run().cells {
            if cell.controller.learner().is_none() {
                assert!(!cell.shows_contingency, "{cell:?}");
                assert_eq!(cell.contingent[1].anticipation, 0);
            }
        }
    }

    #[test]
    fn affordance_discovery_shows_contingency() {
        let report = run();
        for paradigm in PARADIGMS {
            let cell = report.cell(paradigm, Controller::AffordanceDiscovery);
            assert!(cell.shows_contingency, "{paradigm:?}: {cell:?}");
            assert!(
                cell.contingent[1].anticipation > cell.yoked[1].anticipation,
                "{paradigm:?} anticipates caused contact: {cell:?}"
            );
        }
    }

    #[test]
    fn short_delays_are_learned_and_long_delays_are_not() {
        let report = run();
        let delayed = |delay| {
            &report
                .delays
                .iter()
                .find(|row| {
                    row.delay == delay && row.cell.controller == Controller::AffordanceDiscovery
                })
                .expect("every delay is run")
                .cell
        };
        // Short delays are bridged (bouts repeat the cause while its consequence arrives); long
        // delays are not.
        for short in [0, 1, 2] {
            assert!(delayed(short).shows_contingency, "delay {short}");
            assert!(
                delayed(short).contingent[1].anticipation
                    > 4 * delayed(8).contingent[1].anticipation,
                "delay {short}"
            );
        }
        assert!(!delayed(8).shows_contingency);
    }
}
