//! Deterministic reaching test: does the founder learn to push when the object is in front of
//! it, rather than merely that pushing sometimes works?
//!
//! The body can turn between eight directions, and a reachable object stands at eye height in
//! one of them. It is seen on the eye rays toward it, and a hand pulse reaches it only while the
//! founder faces it. Each 40-decision trial starts facing a seed-chosen direction. A context-free
//! learner can only learn that pushing sometimes gives contact; a learner that tells situations
//! apart can learn that it does so when something is close ahead. Nothing names the object, and
//! no controller receives a reward.
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    clippy::missing_panics_doc,
    reason = "compact deterministic experiment harness"
)]

use serde::Serialize;

use crate::harness::{
    self, Controller, Coupling, HAND_PULSE, Object, Phase, PhaseSummary, SEEDS, Sense, Trace, rate,
};

const PHASES: [Phase; 2] = [
    Phase {
        name: "early",
        decisions: 1000,
        coupling: Coupling::Contingent,
    },
    Phase {
        name: "late",
        decisions: 1000,
        coupling: Coupling::Contingent,
    },
];

pub const OBJECT: Object = Object {
    target: HAND_PULSE,
    sense: Sense::Contact,
    resting: 0,
    caused: |_, _| 255,
    delay: 0,
    facing: true,
};

pub const REACHING_CONTROLLERS: [Controller; 4] = [
    Controller::Random,
    Controller::AffordanceDiscovery,
    Controller::ContextDiscovery,
    Controller::ExpectileDiscovery,
];

/// One controller over every seed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Row {
    pub controller: Controller,
    /// Pooled summary of each phase.
    pub phases: Vec<PhaseSummary>,
    /// Late-phase hit rate of each seed, in tenths of a percent of its pushes.
    pub late_hit_rates: Vec<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReachingReport {
    pub schema_version: u16,
    pub seeds: [u64; 8],
    pub phases: [Phase; 2],
    pub rows: Vec<Row>,
}

impl ReachingReport {
    #[must_use]
    pub fn row(&self, controller: Controller) -> &Row {
        self.rows
            .iter()
            .find(|row| row.controller == controller)
            .expect("every reaching controller is run")
    }
}

/// Pushes that reached the object, in tenths of a percent of all pushes.
#[must_use]
pub fn hit_rate(summary: &PhaseSummary) -> u32 {
    let pushes = summary.target(OBJECT);
    (u64::from(summary.consequences) * 1_000 / u64::from(pushes.max(1))) as u32
}

/// Runs every reaching controller over all seeds.
#[must_use]
pub fn run() -> ReachingReport {
    let rows = REACHING_CONTROLLERS
        .iter()
        .map(|controller| {
            let traces: Vec<Trace> = SEEDS
                .iter()
                .map(|seed| harness::run(*controller, &PHASES, OBJECT, *seed, None))
                .collect();
            let late_hit_rates = traces
                .iter()
                .map(|trace| {
                    hit_rate(&harness::summarize(&PHASES, OBJECT, std::slice::from_ref(trace))[1])
                })
                .collect();
            Row {
                controller: *controller,
                phases: harness::summarize(&PHASES, OBJECT, &traces),
                late_hit_rates,
            }
        })
        .collect();
    ReachingReport {
        schema_version: 1,
        seeds: SEEDS,
        phases: PHASES,
        rows,
    }
}

fn percent(tenths: u32) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Compact truthful Markdown rendering for checked-in experiment evidence.
#[must_use]
pub fn markdown(report: &ReachingReport) -> String {
    let mut text = format!(
        "# Reaching test\n\n\
         The body turns between {} directions and a reachable object stands at eye height in \
         one of them; a hand pulse reaches it only while the founder faces it. Each {}-decision \
         trial starts facing a seed-chosen direction. Over {} seeds, {} decisions per phase.\n\n\
         *Facing* is the share of decisions spent facing the object; *pushes while facing* is \
         the share of pushes made then. Pushing on purpose shows as pushes while facing well \
         above facing. *Hit rate* is the share of pushes that reached the object.\n\n\
         | controller | phase | pushes per 100 | facing % | pushes while facing % | hit rate % |\n\
         |---|---|---:|---:|---:|---:|\n",
        harness::HEADINGS,
        harness::FACING_TRIAL,
        report.seeds.len(),
        report.phases[0].decisions
    );
    for row in &report.rows {
        for (phase, summary) in report.phases.iter().zip(&row.phases) {
            let pushes = summary.target(OBJECT);
            text.push_str(&format!(
                "| {:?} | {} | {} | {} | {} | {} |\n",
                row.controller,
                phase.name,
                rate(pushes, summary.decisions),
                rate(summary.facing_decisions, summary.decisions),
                rate(summary.target_while_facing, pushes),
                percent(hit_rate(summary)),
            ));
        }
    }
    text.push_str("\n## Late-phase hit rate per seed (%)\n\n| controller | seeds |\n|---|---|\n");
    for row in &report.rows {
        let seeds: Vec<_> = row
            .late_hit_rates
            .iter()
            .map(|rate| percent(*rate))
            .collect();
        text.push_str(&format!(
            "| {:?} | {} |\n",
            row.controller,
            seeds.join(" · ")
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_deterministic() {
        let a = run();
        let b = run();
        assert_eq!(a.rows, b.rows);
    }

    #[test]
    fn pushes_reach_the_object_only_while_facing_it() {
        for row in run().rows {
            for summary in &row.phases {
                assert_eq!(summary.consequences, summary.target_while_facing, "{row:?}");
            }
        }
    }

    #[test]
    fn telling_situations_apart_improves_aim() {
        let report = run();
        let late = |controller| hit_rate(&report.row(controller).phases[1]);
        assert!(late(Controller::ContextDiscovery) > late(Controller::AffordanceDiscovery));
        assert!(late(Controller::ContextDiscovery) > late(Controller::Random));
    }
}
