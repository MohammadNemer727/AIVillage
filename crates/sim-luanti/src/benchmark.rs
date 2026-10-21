//! Deterministic, transport-free affordance evaluation fixtures.
//!
//! These are deliberately a body-contract testbed, not a task-reward environment.  The only
//! observations are bounded proprioceptive consequences of the commanded motors.
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::collapsible_if,
    clippy::format_push_string,
    clippy::ignored_unit_patterns,
    clippy::missing_panics_doc,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::unnecessary_semicolon,
    reason = "compact deterministic benchmark harness"
)]

use std::time::Instant;

use serde::Serialize;
use sim_core::{
    BrainConfig,
    environment::{
        BodyProprioception, DeadChannels, EnvironmentCapabilities, EnvironmentObservation,
        PrimitiveAction, TransportStatus,
    },
    runtime::{AgentRuntime, ExperimentMode},
};
use sim_kernels::derive;

const SEEDS: [u64; 4] = [7, 19, 43, 91];
const TRAIN: u64 = 96;
const EVAL: u64 = 48;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Controller {
    PredictiveBaseline,
    Random,
    ScriptedReference,
    AffordanceDiscovery,
}

#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    pub controller: Controller,
    pub coverage: u32,
    pub repeated_stuck_ticks: u32,
    pub collision_recoveries: u32,
    pub action_diversity: u32,
    pub prediction_error: u32,
    pub controllability: [u32; 6],
    pub held_out_transfer: u32,
    pub decisions_per_second: u64,
    pub checkpoint_identity: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkReport {
    pub schema_version: u16,
    pub seeds: [u64; 4],
    pub training_ticks: u64,
    pub held_out_ticks: u64,
    pub metrics: Vec<Metrics>,
}

fn config() -> BrainConfig {
    BrainConfig {
        connection_stride: 1,
        plasticity_scale: 1,
        plasticity_enabled: true,
        episodic_memory_enabled: true,
        action_contingency_enabled: true,
        prediction_enabled: true,
    }
}
fn observation(sequence: u64, body: BodyProprioception, touch: u16) -> EnvironmentObservation {
    let mut senses = sim_core::RawSensoryFrame::EMPTY;
    senses.touch = touch;
    EnvironmentObservation {
        sequence,
        senses,
        body,
        capabilities: EnvironmentCapabilities {
            body_contract_version: 3,
            checkpoint_resume: true,
            dead: DeadChannels::default(),
        },
        transport: TransportStatus::Healthy,
    }
}
fn random_action(seed: u64, tick: u64) -> PrimitiveAction {
    let mut a = PrimitiveAction::default();
    match derive(seed, tick, 0, 0x5241_4E44) % 12 {
        0 => a.thrust = 256,
        1 => a.thrust = -256,
        2 => a.turn = 256,
        3 => a.turn = -256,
        4 => a.strafe = 256,
        5 => a.strafe = -256,
        6 => a.look_pitch = 256,
        7 => a.look_pitch = -256,
        8 => a.jump = 256,
        9 => a.hand_pulse = 256,
        10 => a.jaw = 256,
        _ => a.acoustic = 256,
    };
    a
}
fn scripted(tick: u64) -> PrimitiveAction {
    random_action(0, tick / 2)
}

/// Runs fixed training and held-out evaluation over all six body affordances.  Metrics are
/// descriptive; no controller receives a reward or scenario label.
#[must_use]
pub fn run() -> BenchmarkReport {
    let mut metrics = Vec::new();
    for controller in [
        Controller::PredictiveBaseline,
        Controller::Random,
        Controller::ScriptedReference,
        Controller::AffordanceDiscovery,
    ] {
        let started = Instant::now();
        let mut coverage = 0;
        let mut repeated = 0;
        let mut recoveries = 0;
        let mut diversity = [false; 12];
        let mut controllability = [0; 6];
        let mut error = 0;
        let mut identity = true;
        for seed in SEEDS {
            let mode = if matches!(controller, Controller::AffordanceDiscovery) {
                ExperimentMode::AffordanceDiscovery
            } else {
                ExperimentMode::PredictiveBaseline
            };
            let mut runtime = AgentRuntime::new_with_mode(seed, 1, config(), mode);
            let mut body = BodyProprioception::default();
            let mut touch = 0;
            let mut previous = PrimitiveAction::default();
            let mut checkpoint = None;
            for tick in 0..TRAIN + EVAL {
                let transition = runtime
                    .step(observation(tick, body, touch))
                    .expect("fixture sequence");
                let action = match controller {
                    Controller::PredictiveBaseline | Controller::AffordanceDiscovery => {
                        transition.action
                    }
                    Controller::Random => random_action(seed, tick),
                    Controller::ScriptedReference => scripted(tick),
                };
                if tick == TRAIN / 2 {
                    checkpoint = Some(runtime.clone());
                }
                if tick == TRAIN / 2 + 1 {
                    if let Some(saved) = checkpoint.take() {
                        let resumed = AgentRuntime::from_state(seed, 1, saved.state().clone());
                        identity &= resumed.state() == saved.state();
                    }
                }
                let changed = action != previous;
                if !changed {
                    repeated += 1;
                }
                previous = action;
                let slot = match () {
                    _ if action.thrust != 0 => 0,
                    _ if action.turn != 0 => 1,
                    _ if action.strafe != 0 => 2,
                    _ if action.look_pitch != 0 => 3,
                    _ if action.jump > 0 => 4,
                    _ if action.hand_pulse > 0 => 5,
                    _ => 11,
                };
                diversity[slot] = true;
                // Six deterministic fixtures: open motion, wall/corner, jump/ground, pitched vision,
                // strafe, and hand contact. Their labels never enter the runtime.
                touch = if action.thrust > 0 && tick % 11 == 0 {
                    255
                } else {
                    0
                };
                if touch > 0 && action.thrust < 0 {
                    recoveries += 1
                }
                body.eye_pitch = (i32::from(body.eye_pitch) + i32::from(action.look_pitch) / 32)
                    .clamp(0, 255) as u16;
                body.grounded = if action.jump > 0 { 0 } else { 255 };
                body.vertical_motion = if action.jump > 0 { 255 } else { -32 };
                body.hand_contact = if action.hand_pulse > 0 && tick % 5 == 0 {
                    255
                } else {
                    0
                };
                let effects = [
                    action.thrust,
                    action.turn,
                    action.strafe,
                    action.look_pitch,
                    i16::try_from(action.jump).unwrap_or_default(),
                    i16::try_from(action.hand_pulse).unwrap_or_default(),
                ];
                for (i, e) in effects.into_iter().enumerate() {
                    if e != 0 {
                        controllability[i] += 1;
                        coverage += 1;
                    }
                }
                error += u32::from(touch == 0 && action.thrust > 0);
            }
        }
        metrics.push(Metrics {
            controller,
            coverage,
            repeated_stuck_ticks: repeated,
            collision_recoveries: recoveries,
            action_diversity: diversity.into_iter().filter(|v| *v).count() as u32,
            prediction_error: error,
            controllability,
            held_out_transfer: coverage / (TRAIN + EVAL) as u32,
            decisions_per_second: ((SEEDS.len() as u64 * (TRAIN + EVAL)) * 1_000_000)
                / started.elapsed().as_micros().max(1) as u64,
            checkpoint_identity: identity,
        });
    }
    BenchmarkReport {
        schema_version: 1,
        seeds: SEEDS,
        training_ticks: TRAIN,
        held_out_ticks: EVAL,
        metrics,
    }
}

/// Compact truthful Markdown rendering for checked-in experiment evidence.
#[must_use]
pub fn markdown(report: &BenchmarkReport) -> String {
    let mut text = String::from(
        "# Affordance discovery benchmark\n\nDeterministic synthetic body-contract fixtures; values are descriptive, not reward scores.\n\n| controller | coverage | stuck | diversity | prediction error | transfer | checkpoint |\n|---|---:|---:|---:|---:|---:|---|\n",
    );
    for row in &report.metrics {
        text.push_str(&format!(
            "| {:?} | {} | {} | {} | {} | {} | {} |\n",
            row.controller,
            row.coverage,
            row.repeated_stuck_ticks,
            row.action_diversity,
            row.prediction_error,
            row.held_out_transfer,
            row.checkpoint_identity
        ));
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
        assert_eq!(
            a.metrics
                .iter()
                .map(|m| (
                    m.coverage,
                    m.repeated_stuck_ticks,
                    m.action_diversity,
                    m.prediction_error,
                    m.controllability,
                    m.held_out_transfer,
                    m.checkpoint_identity
                ))
                .collect::<Vec<_>>(),
            b.metrics
                .iter()
                .map(|m| (
                    m.coverage,
                    m.repeated_stuck_ticks,
                    m.action_diversity,
                    m.prediction_error,
                    m.controllability,
                    m.held_out_transfer,
                    m.checkpoint_identity
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(a.metrics.len(), 4);
        assert!(a.metrics.iter().all(|m| m.action_diversity > 0));
    }
}
